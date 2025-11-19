use std::{env::current_dir, time::Duration};

use ludis_machine::Machine;
use ludis_system::{Arch, Linux, Os};
use ludis_vm::{run, RunOptions, VmVolume};

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
    let cwd = current_dir().unwrap();
    let volumes = vec![VmVolume {
        source: cwd.join("vm/examples"),
        dest: "/home/debian/test".to_owned(),
    }];
    let command = "echo hello world";
    let timeout = Duration::from_secs(10);
    let options = RunOptions {
        instance_id,
        machine: &machine,
        ports,
        volumes,
        command,
        timeout,
    };
    run(options).await.unwrap();
}
