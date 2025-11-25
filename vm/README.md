# lusid-vm

Develop your lusid config on a virtual machine, before deploying to a physical machine.

## Dependencies

- qemu
- ovmf
- virt-get-kernel (in libguestfs)
- mkisofs (genisoimage)

### Debian

```
sudo apt install qemu-system ovmf libguestfs-tools genisoimage
```

Also:

```shell
sudo usermod -aG kvm $USER
```

## References

- [`cubic-vm/cubic`](https://github.com/cubic-vm/cubic), licensed under MIT and Apache-2.0, Copyright (c) 2025 Roger Knecht
- [`archlinux/vmexec`](https://gitlab.archlinux.org/archlinux/vmexec), licensed under MIT, Copyright (c) 2025 Sven-Hendrik Haase.
