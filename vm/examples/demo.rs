use avo_machine::Machine;
use avo_system::{Arch, Linux, Os};
use avo_vm::{run, VmRunOptions};

#[tokio::main]
async fn main() {
    let machine = Machine {
        hostname: "test".parse().unwrap(),
        os: Os::Linux(Linux::Debian { version: 13 }),
        arch: Arch::X86_64,
        vm: Default::default(),
    };
    let instance_id = machine.hostname.as_ref();
    let command = "echo hello world";
    let options = VmRunOptions {
        ..Default::default()
    };
    run(instance_id, &machine, command, options).await.unwrap();
}
