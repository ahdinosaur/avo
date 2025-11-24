# lusid

_STATUS: PROTOTYPING_

> A modular config system for personal setups

Lusid provisions a fresh computer with the exact setup you need to be productive.

Like .dotfiles on steroids, but less ideological than NixOS. Friendly and functional.

In the past, I've relied on Salt Stack to automate my machine setup, but I think a better way is possible.

See also:

- [comtrya](https://github.com/comtrya/comtrya)
- (legacy) [boxen](https://github.com/boxen/boxen)

## Mutable (impure) configuration

Using [the Rimu language](https://rimu.dev), describe a "plan":

```
name: blah
version: 0.1.0

params:
  whatever:
    type: boolean

setup: (params) =>
  - module: ./stuff
    params:
      stuff: true
  - module: @core/apt
    params:
      package: nvim
  - module: @core/install
    params:
      install: ...
      update: ...
      uninstall: ...
      ...
```

To describe what you see:

- The plan defines basic metadata like name and version (e.g. think `package.json` or `Cargo.toml`)
- The plan defines parameters that it expects to receive
- The plan defines a `setup` function, which return a list of modules to install.
  - These modules can be defined as a user plan in other places, in which case they are called.
  - There is a limited set of core states, which are defined in Rust and called like any other module.

Not shown but is included:

- Like Salt Stack, there is a way to say this happens _before_ or _after_ this.

Not shown but should be included:

- Like Salt Stack, there should be something like "grains" that provide the details of the current system (operating system, etc).
  - This way you can write a block to be generic over any operating system.

As for the execution:

- Given the inputs, the outputs should construct a tree.
  - The branches are user modules, the leaves are core states.
- The core states are evaluated from user-facing params into a sub-tree of atomic resources (each atomic resource representing one thing on your computer).
- For each resource, find the current state of the resource on your computer, then compare with the desired state to determine a resource change.
- Convert each resource change into a sub-tree of operations.
- From the causality tree, find a minimal list of ordered epochs, where each epoch is a list of operations that can be applied together.
- Merge all operations of the same type in the same epoch.
- Iterate through each epoch in order, applying the operations.

## Immutable (pure) builds

We could also have an immutable build system, similar to Nix.

Each build has:

- inputs: from local files or the outputs of other builds
- command: a command to run in a sandboxed directory with the inputs
- outputs: what output files we want to store from the build

## Personal history

1. dotfiles (Ubuntu -> Arch Linux): [`dotfiles2`](https://github.com/ahdinosaur/dotfiles2) / [`dotfiles`](https://github.com/ahdinosaur/dotfiles) / [`dot`](https://github.com/ahdinosaur/dot)
1. CfEngine3 (Gentoo): [`command-center`](https://github.com/ahdinosaur/command-center) / [`blue-dream-masterfiles`](https://github.com/ahdinosaur/blue-dream-masterfiles) / [`bootstraps`](https://github.com/ahdinosaur/bootstraps) / [`dinolay`](https://github.com/ahdinosaur/dinolay)
1. Puppet (Debian): [`dino-puppet`](https://github.com/ahdinosaur/dino-puppet)
1. Salt Stack (Debian): [`ahdinosaur-os`](https://github.com/ahdinosaur-os/config)
1. JavaScript (Regolith, Ubuntu): [`dinos`](https://github.com/ahdinosaur/dinos)
1. more Salt Stack (Regolith, Ubuntu -> Debian): [`dinofarm`](https://github.com/ahdinosaur/dinofarm)
