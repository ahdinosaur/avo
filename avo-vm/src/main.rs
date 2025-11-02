use std::io::IsTerminal;
use std::path::Path;
use std::time::Duration;
use std::{fs, process};

use clap::{CommandFactory, Parser};
use color_eyre::eyre::{bail, Context, Result};
use comfy_table::Table;
use daemonize_me::Daemon;
use dir_lock::DirLock;
use nanoid::nanoid;
use termion::{color, style};
use tokio::runtime::Runtime;
use tracing::{debug, instrument, trace};

mod cli;
mod qemu;
mod runner;
mod ssh;
mod utils;
mod vm_images;

use crate::cli::{
    CleanCommand, Command, ExecCommand, KsmCommand, OsTypeOrImagePath, PsCommand, RunCommand,
    StopCommand,
};
use crate::qemu::{convert_ovmf_uefi_variables, QemuLaunchOpts};
use crate::ssh::{connect_ssh_for_command, ensure_ssh_key, SshLaunchOpts};
use crate::utils::get_live_cid_and_pids_for_vmid;
use crate::utils::{
    check_ksm_active, create_free_cid, find_required_tools, install_tracing, print_ksm_stats,
    reap_dead_run_dirs, VmexecDirs, HEX_ALPHABET,
};
use crate::vm_images::VmImage;

#[instrument]
fn ksm_command(ksm_args: KsmCommand) -> Result<()> {
    if let Some(enable_disable) = ksm_args.ksm_enable_disable {
        if whoami::username() != "root" {
            bail!("You need to run this particular subcommand as root");
        }

        if enable_disable.enable {
            println!("Writing KSM config to /etc/tmpfiles.d/ksm.conf and reloading systemd");
            let ksm_conf = "\
w /sys/kernel/mm/ksm/run - - - - 1
w /sys/kernel/mm/ksm/advisor_mode - - - - scan-time
";
            fs::write("/etc/tmpfiles.d/ksm.conf", ksm_conf)?;
            process::Command::new("systemd-tmpfiles")
                .arg("--create")
                .output()?;
        } else if enable_disable.disable {
            println!("Removing KSM config at /etc/tmpfiles.d/ksm.conf and reloading systemd");
            fs::write("/sys/kernel/mm/ksm/run", "0")?;
            fs::write("/sys/kernel/mm/ksm/advisor_mode", "none")?;
            fs::remove_file("/etc/tmpfiles.d/ksm.conf")?;
        }
    } else {
        let ksm_enabled = fs::read_to_string("/sys/kernel/mm/ksm/run")?.trim() == "1";
        if ksm_enabled {
            println!(
                "{}KSM status: {}enabled{}",
                style::Bold,
                color::Fg(color::LightGreen),
                style::Reset,
            );
            print_ksm_stats()?;
        } else {
            println!(
                "{}KSM status: {}disabled{}",
                style::Bold,
                color::Fg(color::Yellow),
                style::Reset
            );
        }
    }
    Ok(())
}

#[instrument]
fn clean_command(_clean_args: CleanCommand) -> Result<()> {
    let dirs = VmexecDirs::new()?;
    let cleaned_run_dirs = reap_dead_run_dirs(&dirs.runs_dir)?;
    if !cleaned_run_dirs.is_empty() {
        println!(
            "Removed old runs:\n  {}",
            cleaned_run_dirs
                .iter()
                .map(|dir| dir.to_string_lossy())
                .collect::<Vec<_>>()
                .join("\n  ")
        );
    } else {
        println!("Nothing to do!");
    }

    Ok(())
}

#[instrument]
fn ps_command(_ps_args: PsCommand) -> Result<()> {
    let dirs = VmexecDirs::new()?;

    let lock_dir = dirs.cache_dir.join("lockdir");
    trace!("Trying to lock {lock_dir:?}");
    let lock = DirLock::new_sync(&lock_dir)?;

    let mut table = Table::new();
    table.load_preset(comfy_table::presets::NOTHING);
    table.set_header(["VM ID", "IMAGE", "COMMAND", "CREATED", "STATUS", "PORTS"]);

    for column in table.column_iter_mut() {
        column.set_padding((0, 2));
    }

    let entries = fs::read_dir(&dirs.runs_dir)?;
    for entry in entries {
        let dir = entry?.path();

        if dir.is_dir() {
            let qemu_launch_opts_str = fs::read_to_string(dir.join("qemu_launch_opts"))?;
            let qemu_launch_opts: QemuLaunchOpts = serde_json::from_str(&qemu_launch_opts_str)?;
            let ssh_launch_opts_str = fs::read_to_string(dir.join("ssh_launch_opts"))?;
            let ssh_launch_opts: SshLaunchOpts = serde_json::from_str(&ssh_launch_opts_str)?;
            let created_at_secs = fs::metadata(dir.join("qemu.pid"))?
                .created()?
                .elapsed()?
                .as_secs();
            let created_at_duration = Duration::from_secs(created_at_secs);
            let created_at = humantime::format_duration(created_at_duration).to_string();
            let created_at = format!("{created_at} ago");

            let vmid = qemu_launch_opts.vmid;
            let status = if get_live_cid_and_pids_for_vmid(&vmid, &dirs.runs_dir)?
                .qemu_pid
                .is_some()
            {
                "Running".to_string()
            } else {
                "Exited".to_string()
            };

            let image = qemu_launch_opts
                .vm_image
                .image_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            let command = ssh_launch_opts.args.join(" ");
            let ports = qemu_launch_opts
                .published_ports
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<String>>()
                .join(", ");

            table.add_row(&[vmid, image, command, created_at, status, ports]);
        }
    }

    println!("{table}");

    trace!("Unlocking {lock_dir:?}");
    drop(lock);

    Ok(())
}

fn stop_command(stop_args: StopCommand) -> Result<()> {
    let dirs = VmexecDirs::new()?;

    let cid_pids = get_live_cid_and_pids_for_vmid(&stop_args.vmid, &dirs.runs_dir)?;
    let qemu_pid = cid_pids
        .qemu_pid
        .expect("Somehow vmexec.pid doesn't refer to a live process");
    let vmexec_pid = cid_pids
        .vmexec_pid
        .expect("Somehow vmexec.pid doesn't refer to a live process");

    debug!("Found {vmexec_pid} to be live, sending kill signal");
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(vmexec_pid as i32),
        nix::sys::signal::SIGTERM,
    )?;

    debug!("Found {qemu_pid} to be live, sending kill signal");
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(qemu_pid as i32),
        nix::sys::signal::SIGTERM,
    )?;

    Ok(())
}

#[instrument]
fn exec_command(exec_args: ExecCommand) -> Result<()> {
    let dirs = VmexecDirs::new()?;

    let ssh_keypair = ensure_ssh_key(&dirs.secrets_dir)?;
    let cid = get_live_cid_and_pids_for_vmid(&exec_args.vmid, &dirs.runs_dir)?.cid;

    let tty = match exec_args.tty {
        cli::Tty::Always => true,
        cli::Tty::Never => false,
        cli::Tty::Auto => std::io::stdout().is_terminal(),
    };

    debug!(
        "tty: {tty}, interactive: {interactive:?}",
        interactive = exec_args.interactive
    );

    let ssh_launch_opts = ssh::SshLaunchOpts {
        timeout: exec_args.ssh_timeout,
        tty,
        interactive: exec_args.interactive,
        env_vars: exec_args.env,
        args: exec_args.args,
        privkey: ssh_keypair.privkey_str,
        cid,
    };

    let rt = Runtime::new()?;
    rt.block_on(async { connect_ssh_for_command(ssh_launch_opts).await })?;
    Ok(())
}

#[instrument]
fn run_command(run_args: RunCommand) -> Result<Option<u32>> {
    // Make sure the tools we need are actually installed.
    let tool_paths = find_required_tools()?;

    // Check whether KSM is active.
    check_ksm_active()?;

    let dirs = VmexecDirs::new()?;

    // Lock the runs dir here. We have to keep the lock around until QEMU has launched for this
    // run. If we unlock too early (before there's a live qemu.pid) then a second launch that
    // happens right after this one might try to clean up the "dead" dir. We don't want this race
    // condition.
    let lock_dir = dirs.cache_dir.join("lockdir");
    trace!("Trying to lock {lock_dir:?}");
    let lock = DirLock::new_sync(&lock_dir)?;

    // Dir for this run (usually ~/.local/share/vmexec/runs/<random id>/)
    // Supposed to get cleaned up after QEMU stops running.
    let vmid = nanoid!(12, &HEX_ALPHABET);
    let run_dir = Path::new(&dirs.runs_dir).join(&vmid);
    fs::create_dir(&run_dir).wrap_err(format!("Couldn't make temp dir in {:?}", dirs.runs_dir))?;
    debug!("run dir is: {:?}", run_dir);

    let pid_file = run_dir.join("vmexec.pid");
    if run_args.detach {
        // In case the user chooses to detach this, we'll daemonize at this point. That means the main
        // process will exit while the child process will continue running the commands.

        debug!("Detach was requested so we're now daemonizing");
        println!("{vmid}");

        let stdout = fs::File::create(run_dir.join("daemon.stdout"))?;
        let stderr = fs::File::create(run_dir.join("daemon.stderr"))?;
        Daemon::new()
            .pid_file(pid_file, None)
            .stdout(stdout)
            .stderr(stderr)
            .setup_post_fork_parent_hook(post_fork_parent)
            .start()?;
    } else {
        // If we're not daemonizing, we're just writing our pid out and that's it.

        fs::write(pid_file, std::process::id().to_string())?;
    }

    let rt = Runtime::new()?;
    let vm_image = rt.block_on(async {
        match run_args.image_source {
            OsTypeOrImagePath::OsType(os_type) => match os_type {
                cli::OsType::Archlinux => VmImage::archlinux(&dirs.cache_dir, run_args.pull).await,
            },
            OsTypeOrImagePath::ImagePath(image_path) => Ok(VmImage::generic(&image_path)),
        }
    })?;

    let ssh_keypair = ensure_ssh_key(&dirs.secrets_dir)?;

    // We need a free CID for host-guest communication via vsock.
    let cid = create_free_cid(&dirs.runs_dir, &run_dir)?;

    // let ovmf_vars_system_path = Path::new("/usr/share/edk2/x64/OVMF_VARS.4m.fd");
    let ovmf_vars_system_path = Path::new("/usr/share/OVMF/OVMF_VARS_4M.fd");
    let ovmf_vars =
        rt.block_on(async { convert_ovmf_uefi_variables(&run_dir, ovmf_vars_system_path).await })?;

    let qemu_launch_opts = qemu::QemuLaunchOpts {
        vmid,
        volumes: run_args.volumes,
        pmems: run_args.pmems,
        published_ports: run_args.published_ports,
        vm_image: vm_image.clone(),
        ovmf_uefi_vars_path: ovmf_vars,
        show_vm_window: run_args.show_vm_window,
        pubkey: ssh_keypair.pubkey_str,
        cid,
        is_warmup: true,
        disable_kvm: run_args.disable_kvm,
    };

    let tty = match run_args.tty {
        cli::Tty::Always => true,
        cli::Tty::Never => false,
        cli::Tty::Auto => std::io::stdout().is_terminal(),
    };

    debug!(
        "tty: {tty}, interactive: {interactive:?}",
        interactive = run_args.interactive
    );

    let ssh_launch_opts = ssh::SshLaunchOpts {
        timeout: run_args.ssh_timeout,
        tty,
        interactive: run_args.interactive,
        env_vars: run_args.env,
        args: run_args.args,
        privkey: ssh_keypair.privkey_str,
        cid,
    };

    let exit_code = rt.block_on(async {
        let overlay_image_path = runner::warmup(
            &run_dir,
            tool_paths.clone(),
            qemu_launch_opts.clone(),
            ssh_launch_opts.clone(),
        )
        .await?;

        // We create a new `QemuLaunchOpts` here so that we can launch QEMU from the overlay image
        // instead of the source image.
        let vm_image = VmImage {
            image_path: overlay_image_path,
            ..vm_image
        };
        let qemu_launch_opts = qemu::QemuLaunchOpts {
            vm_image,
            is_warmup: false,
            ..qemu_launch_opts
        };

        debug!("SSH command for manual debugging:");
        debug!(
            "ssh root@vsock/{cid} -i {privkey_path:?}",
            privkey_path = ssh_keypair.privkey_path
        );
        runner::run(
            &run_dir,
            lock,
            tool_paths,
            qemu_launch_opts,
            ssh_launch_opts,
        )
        .await
    })?;

    // Clean up after running in case this has been requested.
    if run_args.rm {
        debug!("Removing run dir at {run_dir:?}");
        fs::remove_dir_all(&run_dir)?;
    }

    Ok(exit_code)
}

// As a general note, the reason this program uses mixed sync and async is because we optionally daemonize.
// The program will misbehave if the tokio runtime is already running at the time of daemonizing.
// See for instance https://github.com/xadaemon/daemonize-me/discussions/16
// This requires that we run the program sync until after daemonizing. This is fine since clap
// argument parsing can't really be async anyway.
fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    install_tracing(cli.log_level);
    color_eyre::install()?;

    match cli.command {
        Command::Ksm(ksm_args) => ksm_command(ksm_args)?,
        Command::Clean(clean_args) => clean_command(clean_args)?,
        Command::Ps(ps_args) => ps_command(ps_args)?,
        Command::Stop(stop_args) => stop_command(stop_args)?,
        Command::Exec(exec_args) => exec_command(exec_args)?,
        Command::Run(run_args) => {
            let exit_code = run_command(run_args)?;
            if let Some(child_code) = exit_code {
                std::process::exit(child_code.try_into()?);
            }
        }
        Command::Completions { shell } => {
            let mut clap_app = cli::Cli::command();
            let app_name = clap_app.get_name().to_string();
            clap_complete::generate(shell, &mut clap_app, app_name, &mut std::io::stdout());
        }
        Command::Manpage { out_dir } => {
            let clap_app = cli::Cli::command();
            clap_mangen::generate_to(clap_app, out_dir)?;
        }
    }
    Ok(())
}

fn post_fork_parent(_ppid: i32, _cpid: i32) -> ! {
    process::exit(0);
}
