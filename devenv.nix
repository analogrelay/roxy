{ pkgs, lib, config, inputs, ... }:

{
  packages = [ 
    pkgs.git
    pkgs.qemu
    pkgs.lld
    pkgs.rustfilt
  ];
  languages.rust = {
    enable = true;
    channel = "nightly";
    components = [
      "rust-src"
    ];
    targets = [
      "x86_64-unknown-none"
    ];
  };
}
