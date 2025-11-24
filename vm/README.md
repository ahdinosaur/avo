# lusid-vm

Develop your lusid config on a virtual machine, before deploying to a physical machine.

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
- virt-get-kernel (in libguestfs)
- mkisofs (genisoimage)
- unshare

### Debian

```
sudo apt install qemu-system virtiofsd ovmf libguestfs-tools genisoimage unshare
```

If on Debian Bookworm, install `virtiofsd` with `cargo install`.

Also:

```shell
sudo usermod -aG kvm $USER
```

## References

- [`cubic-vm/cubic`](https://github.com/cubic-vm/cubic), licensed under MIT and Apache-2.0, Copyright (c) 2025 Roger Knecht
- [`archlinux/vmexec`](https://gitlab.archlinux.org/archlinux/vmexec), licensed under MIT, Copyright (c) 2025 Sven-Hendrik Haase.

## Implementation Notes

Steps to run a command in QEMU :

1. Download the base image
2. Prep image
    1. Convert OVMF UEFI variables
    2. Extract kernel from image
3. Warmup run with base image, save snapshot as overlay
4. Normal run with overlay image
