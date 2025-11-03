# avo-vm

Develop your avo config on a virtual machine, before deploying to a physical machine.

- List images
- Create new machine with id and config
- List machines by id
- Start cached machine from id

Machine configuration:

- Name
- Base image
  - Debian
  - Arch Linux
- Memory (default to total memory on host)
- Cpu logical cores (defaults to total CPU cores on host)
- Ssh key

## Dependencies

- qemu
- virtiofsd
- virt-copy-out (in libguestfs)

### Debian

```
sudo apt install qemu-system virtiofsd ovmf libguestfs-tools
```

If on Debian Bookworm, install `virtiofsd` with `cargo install`.

## References

- [`cubic-vm/cubic`](https://github.com/cubic-vm/cubic), licensed under MIT and Apache-2.0, Copyright (c) 2025 Roger Knecht
- [`archlinux/vmexec`](https://gitlab.archlinux.org/archlinux/vmexec), licensed under MIT, Copyright (c) 2025 Sven-Hendrik Haase.

## Implementation Notes

Steps to run a command in QEMU :

1. Download the base image
2.
