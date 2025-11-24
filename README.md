# lusid

_STATUS: PROTOTYPING_

> A modular config system for personal setups

Lusid provisions a fresh computer with the exact setup you need to be productive.

Like .dotfiles on steroids, but less opinionated than NixOS. Friendly and functional.

In the past, I've relied on Salt Stack to automate my machine setup, but I think a better way is possible.

See also:

- [comtrya](https://github.com/comtrya/comtrya)
- (legacy) [boxen](https://github.com/boxen/boxen)

## Mutable (impure) configuration

Using [the Rimu language](https://rimu.dev), describe a "configuration block":

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
  - module: @core/pkg
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

- The block defines basic metadata like name and version (e.g. think `package.json` or `Cargo.toml`)
- The block defines parameters that it expects to receive
- The block defines a `setup` function, which return a list of modules to install.
  - These modules can be defined in other places, in which case they are called.
  - There is a limited set of core operations, which are called like any other module.

Not shown but should be included:

- Like Salt Stack, there should be something like "grains" that provide the details of the current system (operating system, etc).
  - This way you can write a block to be generic over any operating system.
- Like Salt Stack, there should be a way to say this happens _before_ or _after_ this.

As for the execution:

- Given the inputs, the outputs should construct a tree.
  - The branches are user modules, the leaves are core operations.
- Then the tree can be constructed into a graph, where nodes are ordered based on parent-child or dependency relationships.
  - This is a causality graph, so nodes that could happen at the same time should be grouped together.
  - Each core operation should then be able to reduce a group of operations at the same causality into a single operation.
- Then you should be able to view the tree in something like [ratatui](https://ratatui.rs/)
  - See the progress of each node
  - See the output of each leaf

## Immutable (pure) packages

We could also have a

## Personal history

1. dotfiles (Ubuntu -> Arch Linux): [`dotfiles2`](https://github.com/ahdinosaur/dotfiles2) / [`dotfiles`](https://github.com/ahdinosaur/dotfiles) / [`dot`](https://github.com/ahdinosaur/dot)
1. CfEngine3 (Gentoo): [`command-center`](https://github.com/ahdinosaur/command-center) / [`blue-dream-masterfiles`](https://github.com/ahdinosaur/blue-dream-masterfiles) / [`bootstraps`](https://github.com/ahdinosaur/bootstraps) / [`dinolay`](https://github.com/ahdinosaur/dinolay)
1. Puppet (Debian): [`dino-puppet`](https://github.com/ahdinosaur/dino-puppet)
1. Salt Stack (Debian): [`ahdinosaur-os`](https://github.com/ahdinosaur-os/config)
1. JavaScript (Regolith, Ubuntu): [`dinos`](https://github.com/ahdinosaur/dinos)
1. more Salt Stack (Regolith, Ubuntu -> Debian): [`dinofarm`](https://github.com/ahdinosaur/dinofarm)
