use clap::Parser;

#[derive(Parser)]
struct Args {
    #[clap(short, long)]
    debug: bool,

    #[clap(long)]
    bios: bool,

    #[clap(long, short)]
    memory: Option<String>,
}

fn main() {
    let args = Args::parse();

    // read env variables that were set in build script
    let uefi_path = env!("UEFI_PATH");
    let bios_path = env!("BIOS_PATH");

    let mut cmd = std::process::Command::new("qemu-system-x86_64");
    if args.bios {
        cmd.arg("-drive")
            .arg(format!("format=raw,file={bios_path}"));
    } else {
        cmd.arg("-bios").arg(ovmf_prebuilt::ovmf_pure_efi());
        cmd.arg("-drive")
            .arg(format!("format=raw,file={uefi_path}"));
    }

    if args.debug {
        cmd.arg("-s")
            .arg("-S")
            .arg("-no-reboot")
            .arg("-no-shutdown");
    }

    if let Some(memory) = args.memory {
        cmd.arg("-m").arg(memory);
    }

    let mut child = cmd.spawn().unwrap();
    child.wait().unwrap();
}
