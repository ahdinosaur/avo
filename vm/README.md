# avo-vm

Develop your avo config on a virtual machine, before deploying to a physical machine.

- Create new machine with id
- List machines by id
- Start cached machine from id

Machine configuration:

- Base OS
  - Debian
  - Arch Linux

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
