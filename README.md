# typst-package-review

Helper for testing packages submitted to typst universe.

## Setup
1. Clone `https://github.com/typst/packages` into this directory
    - This currently only supports `http`
2. Compile and install `typst-package-review` by running `cargo install --path review`

## Usage
Now you can simply copy and paste a github PR title and enjoy.
Depending on your shell you might need to use quotes so the `#` symbol isn't interpreted as a comment.
Here is an example:
```
typst-package-review haw-hamburg-bachelor-thesis:0.6.2, haw-hamburg-master-thesis:0.6.2, haw-hamburg-report:0.6.2 and haw-hamburg:0.6.2 #3173
```

This tool will automatically:
- Fetch the pull request into a local branch
- Install the packages locally in the `preview` namespace
- Initialize templates if the templates have some
- Try to find an entry point for a template and compile it

```
Review PR #3173
  haw-hamburg-bachelor-thesis v0.6.2
  haw-hamburg-master-thesis v0.6.2
  haw-hamburg-report v0.6.2
  haw-hamburg v0.6.2

=== Fetch ===
fetching pull/3173/head
checkout haw-hamburg-bachelor-thesis_0.6.2,haw-hamburg-master-thesis_0.6.2,haw-hamburg-report_0.6.2,haw-hamburg_0.6.2_#3173

=== Install ===
install packages/packages/preview/haw-hamburg-bachelor-thesis/0.6.2
install packages/packages/preview/haw-hamburg-master-thesis/0.6.2
install packages/packages/preview/haw-hamburg-report/0.6.2
install packages/packages/preview/haw-hamburg/0.6.2
initialize template @preview/haw-hamburg-bachelor-thesis:0.6.2
Successfully created new project from @preview/haw-hamburg-bachelor-thesis:0.6.2 🎉
To start writing, run:
> cd test/haw-hamburg-bachelor-thesis
> typst watch main.typ

compile template test/haw-hamburg-bachelor-thesis/main.typ
initialize template @preview/haw-hamburg-master-thesis:0.6.2
Successfully created new project from @preview/haw-hamburg-master-thesis:0.6.2 🎉
To start writing, run:
> cd test/haw-hamburg-master-thesis
> typst watch main.typ

compile template test/haw-hamburg-master-thesis/main.typ
initialize template @preview/haw-hamburg-report:0.6.2
Successfully created new project from @preview/haw-hamburg-report:0.6.2 🎉
To start writing, run:
> cd test/haw-hamburg-report
> typst watch main.typ

compile template test/haw-hamburg-report/main.typ
```
