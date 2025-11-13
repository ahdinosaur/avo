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
    let options = VmRunOptions {
        command: "echo hello world".into(),
        ..Default::default()
    };
    run(machine, options).await.unwrap();
}
