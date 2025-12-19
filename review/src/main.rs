use anyhow::{Context, bail};
use git2::{BranchType, FetchOptions, Repository};
use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use typst_syntax::package::PackageManifest;

const ANSII_RED: &str = "\x1b[31m";
const ANSII_GREEN: &str = "\x1b[32m";
const ANSII_YELLOW: &str = "\x1b[33m";
const ANSII_BLUE: &str = "\x1b[34m";
const ANSII_CLEAR: &str = "\x1b[0m";

struct Args<'a> {
    packages: Vec<Package<'a>>,
    pr_nr: u32,
}

impl Args<'_> {
    fn branch_name(&self) -> String {
        let Args { packages, pr_nr } = self;
        let mut buf = String::new();
        for (i, Package { name, vers }) in packages.iter().enumerate() {
            if i > 0 {
                buf.push(',');
            }
            _ = write!(&mut buf, "{name}_{vers}");
        }
        _ = write!(&mut buf, "_#{pr_nr}");
        buf
    }
}

#[derive(Debug)]
struct Package<'a> {
    name: &'a str,
    vers: &'a str,
}

impl Package<'_> {
    fn spec(&self) -> String {
        let Package { name, vers } = self;
        format!("@preview/{name}:{vers}")
    }
}

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("{e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[derive(Clone, Copy)]
enum Cmd {
    Review,
    Fetch,
    Install,
}

impl Cmd {
    fn fetch(&self) -> bool {
        match self {
            Cmd::Review | Cmd::Fetch => true,
            Cmd::Install => false,
        }
    }

    fn install(&self) -> bool {
        match self {
            Cmd::Review | Cmd::Install => true,
            Cmd::Fetch => false,
        }
    }
}

fn run() -> anyhow::Result<()> {
    let mut args = std::env::args();
    args.next();
    let Some(cmd) = args.next() else {
        bail!("missing command");
    };

    let cmd = match cmd.as_str() {
        "review" => Cmd::Review,
        "fetch" => Cmd::Fetch,
        "install" => Cmd::Install,
        "clean" => return clean(),
        _ => bail!("unknown command `{cmd}`"),
    };

    let args = args.collect::<Vec<_>>().join(" ");
    let args: Vec<_> = args.split(' ').filter(|s| !s.is_empty()).collect();
    let args = parse_args(&args)?;

    let Args { packages, pr_nr } = &args;
    println!("PR {ANSII_YELLOW}#{pr_nr}{ANSII_CLEAR}");
    for Package { name, vers } in packages.iter() {
        println!("  {ANSII_BLUE}{name}{ANSII_CLEAR} v{vers}");
    }
    println!();

    if cmd.fetch() {
        println!("=== Fetch ===");
        checkout_pr(&args)?;
        println!();
    }

    let mut res = Ok(());
    if cmd.install() {
        println!("=== Install ===");
        let manifests = (packages.iter())
            .map(install_package)
            .collect::<Result<Vec<_>, _>>()?;
        println!();

        println!("=== Test ===");
        std::fs::create_dir_all("test").context("failed to create `test` directory")?;
        for (package, manifest) in packages.iter().zip(manifests.iter()) {
            let r = test_package(package, manifest);
            if res.is_ok() {
                res = r;
            }
        }
    }

    res
}

fn parse_args<'a>(args: &[&'a str]) -> anyhow::Result<Args<'a>> {
    if args.len() < 2 {
        bail!("expected at least one package and the PR number");
    }
    let (pr_nr, args) = args.split_last().unwrap();
    let Some(pr_nr) = pr_nr.strip_prefix("#") else {
        bail!("PR number must start with `#` - `{pr_nr}`");
    };
    let Ok(pr_nr) = pr_nr.parse() else {
        bail!("PR number is not valid - `{pr_nr}`");
    };

    let mut packages = Vec::with_capacity(args.len());
    for arg in args.iter() {
        let arg = arg.trim_end_matches(',');
        if arg == "and" {
            continue;
        }

        let Some((name, vers)) = arg.split_once(':') else {
            bail!("package name and version must be separated by `:` - `{arg}`");
        };
        packages.push(Package { name, vers });
    }

    Ok(Args { packages, pr_nr })
}

fn checkout_pr(args @ Args { pr_nr, .. }: &Args) -> anyhow::Result<()> {
    let branch_name = &args.branch_name();

    let repo = Repository::open("packages")?;

    // Make sure we're on the `main` branch.
    if repo.head()?.name() != Some("main") {
        checkout_branch(&repo, "main")?;
    }

    // Make sure the branch doesn't exist
    let local_branches = repo.branches(Some(BranchType::Local))?;
    for b in local_branches {
        let (mut branch, _) = b?;
        if branch.name()? == Some(branch_name) {
            println!("remove existing branch {ANSII_RED}{branch_name}{ANSII_CLEAR}");
            branch.delete()?;
            break;
        }
    }

    // Fetch the PR branch into a local branch.
    let mut origin = repo.find_remote("origin")?;
    let refspec = format!("pull/{pr_nr}/head");
    println!("fetching {ANSII_YELLOW}{refspec}{ANSII_CLEAR}");
    let mut fetch_opts = FetchOptions::new();
    origin.fetch(&[refspec], Some(&mut fetch_opts), None)?;

    // Find the commit of the PR.
    let head_name = format!("refs/pull/{pr_nr}/head");
    let fetch_head = origin
        .list()?
        .iter()
        .find(|h| h.name() == head_name)
        .expect("remote head after we successfully fetched it");
    let commit = repo.find_commit(fetch_head.oid())?;

    // Create a branch with the commit.
    println!("checkout {ANSII_YELLOW}{branch_name}{ANSII_CLEAR}");
    repo.branch(branch_name, &commit, true)?;

    // Check it out.
    checkout_branch(&repo, branch_name)?;

    Ok(())
}

fn checkout_branch(repo: &Repository, branch_name: &str) -> Result<(), git2::Error> {
    let (obj, refname) = repo.revparse_ext(branch_name)?;
    repo.checkout_tree(&obj, None)?;
    if let Some(refname) = refname {
        repo.set_head(refname.name().expect("valid name"))?;
    }
    Ok(())
}

fn install_package(Package { name, vers }: &Package) -> anyhow::Result<PackageManifest> {
    let package_dir = PathBuf::from_iter(["packages", "packages", "preview", name, vers]);
    let mut target_dir = dirs::data_dir().expect("data dir");
    target_dir.extend(["typst", "packages", "preview", name, vers]);

    println!(
        "install {ANSII_YELLOW}{}{ANSII_CLEAR}",
        package_dir.display()
    );

    // Read manifest.
    let manifest_path = package_dir.join("typst.toml");
    let manifest =
        std::fs::read_to_string(manifest_path).context("failed to read package manifest")?;
    let manifest: PackageManifest =
        toml::from_str(&manifest).context("failed to parse package manifest")?;

    // Build exclude overrides.
    let mut builder = OverrideBuilder::new(&package_dir);
    for exclude in manifest.package.exclude.iter() {
        if exclude.starts_with('!') {
            bail!("exclude globs cannot start with `!` - `{exclude}`");
        }
        let exclude = exclude.trim_start_matches("./");
        let inverted = format!("!{exclude}");
        builder.add(&inverted).context("invalid exclude glob")?;
    }
    let excludes = builder.build()?;
    let walk = WalkBuilder::new(&package_dir).overrides(excludes).build();

    // Delete existing package
    if target_dir.exists() {
        println!(
            "remove existing package {ANSII_RED}{}{ANSII_CLEAR}",
            target_dir.display()
        );
        std::fs::remove_dir_all(&target_dir).context("failed to remove existing package")?;
    }

    // Copy files over
    for entry in walk.into_iter() {
        let entry = entry.context("failed to traverse")?;

        let relative_path = entry
            .path()
            .strip_prefix(&package_dir)
            .expect("path to be relative to package dir");
        let target_path = target_dir.join(relative_path);

        if entry.file_type().is_some_and(|f| f.is_file()) {
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!("failed to create parent directory `{}`", parent.display())
                })?;
            }

            std::fs::copy(entry.path(), &target_path)
                .with_context(|| format!("failed to copy to `{}`", target_path.display()))?;
        }
    }

    Ok(manifest)
}

fn test_package(
    package @ Package { name, .. }: &Package,
    manifest: &PackageManifest,
) -> anyhow::Result<()> {
    if let Some(template) = &manifest.template {
        // Initialize template
        let spec = &package.spec();
        println!("initialize template {ANSII_GREEN}{spec}{ANSII_CLEAR}");

        let template_dir = PathBuf::from_iter(["test", name]);
        if template_dir.exists() {
            println!(
                "remove existing template {ANSII_RED}{}{ANSII_CLEAR}",
                template_dir.display()
            );
            std::fs::remove_dir_all(&template_dir).context("failed to remove existing template")?;
        }

        run_command(
            "typst",
            ["init", spec, template_dir.to_str().expect("valid ASCII")],
        )?;

        // Try to compile template.
        let entrypoint = template_dir.join(template.entrypoint.as_str());
        let entrypoint_str = entrypoint.to_str().expect("valid utf-8");
        println!("compile template {ANSII_GREEN}{entrypoint_str}{ANSII_CLEAR}");
        run_command("typst", ["compile", entrypoint_str])?;

        // Open the PDF
        let pdf = entrypoint.with_extension("pdf");
        let pdf_str = pdf.to_str().expect("valid utf-8");
        run_command("xdg-open", [pdf_str])?;
    }

    Ok(())
}

fn run_command<const N: usize>(cmd: &str, args: [&str; N]) -> anyhow::Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .status()
        .expect("failed to execute process");

    if !status.success() {
        bail!("command failed");
    }

    Ok(())
}

fn clean() -> anyhow::Result<()> {
    let mut target_dir = dirs::data_dir().expect("data dir");
    target_dir.extend(["typst", "packages", "preview"]);
    clear_directory(&target_dir).context("failed to clean target directory")?;
    clear_directory("test".as_ref()).context("failed to clean target directory")?;
    remove_other_branches().context("failed to clean branches")?;
    Ok(())
}

fn clear_directory(dir: &Path) -> anyhow::Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        println!("directory wasn't found at: `{}`", dir.display());
        return Ok(());
    };
    for entry in entries {
        let entry = entry.context("failed to read entry")?;
        println!("remove {ANSII_RED}{}{ANSII_CLEAR}", entry.path().display());
        let file_type = entry.file_type().context("failed to read entry")?;
        if file_type.is_dir() {
            std::fs::remove_dir_all(entry.path()).context("failed to remove directory")?;
        } else {
            std::fs::remove_file(entry.path()).context("failed to remove file")?;
        }
    }
    Ok(())
}

fn remove_other_branches() -> Result<(), git2::Error> {
    let repo = Repository::open("packages")?;

    // Make sure we're on the `main` branch.
    if repo.head()?.name() != Some("main") {
        checkout_branch(&repo, "main")?;
    }

    // Make sure the branch doesn't exist
    let local_branches = repo.branches(Some(BranchType::Local))?;
    for b in local_branches {
        let (mut branch, _) = b?;
        let Some(branch_name) = branch.name()? else {
            continue;
        };

        if branch_name != "main" {
            println!("remove branch {ANSII_RED}{branch_name}{ANSII_CLEAR}");
            branch.delete()?;
        }
    }

    Ok(())
}
