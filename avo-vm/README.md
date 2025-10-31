# avo-vm

Develop your avo config on a virtual machine, before deploying to a physical machine.

Initial code copied from [`archlinux/vmexec`](https://gitlab.archlinux.org/archlinux/vmexec), licensed under MIT, Copyright (c) 2025 Sven-Hendrik Haase.

- Create new machine with id
- List machines by id
- Start cached machine from id

Machine configuration:

- Base OS
  - Debian
  - Arch Linux

Stack:

- qemu
- virtiofsd
- virtio-gpu
