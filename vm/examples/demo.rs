use avo_machine::Machine;
use avo_system::{Arch, Linux, Os};
use avo_vm::{run, RunOptions};

#[tokio::main]
async fn main() {
    let machine = Machine {
        hostname: "test".parse().unwrap(),
        os: Os::Linux(Linux::Debian { version: 13 }),
        arch: Arch::X86_64,
        vm: Default::default(),
    };
    let instance_id = machine.hostname.as_ref();
    let ports = vec![];
    let volumes = vec![];
    let command = "echo hello world";
    let options = RunOptions {
        instance_id,
        machine: &machine,
        ports,
        volumes,
        command,
    };
    run(options).await.unwrap();
}
