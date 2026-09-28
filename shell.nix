
{ pkgs ? import <nixpkgs> {} }:

let
 
  llvm = pkgs.llvmPackages_latest; #

in
pkgs.mkShell {
  name = "fframes-dev-env";

  buildInputs = [
    pkgs.x264       
    pkgs.x265           
    pkgs.libopus 
    llvm.libclang
    pkgs.libgcc
    pkgs.ffmpeg     
  ];

  nativeBuildInputs = [
    pkgs.ninja
    pkgs.pkg-config 
    pkgs.yasm      
    pkgs.nasm       
    llvm.clang      
    pkgs.nodejs     
    pkgs.pnpm      
    pkgs.rustup      
    pkgs.just       
    pkgs.cargo-watch 
    pkgs.wasm-bindgen-cli 
    pkgs.wasm-pack  
  ];


  LIBCLANG_PATH = "${llvm.libclang.lib}/lib";

  shellHook = ''
    echo ""
    echo "--------------------------------------------------"
    echo " Entering development shell for fframes 🎬"
    echo "   'Write some Rust. Get video. Enjoy 🥤🍿'"
    echo "--------------------------------------------------"
    echo "   If this is your first time setting up the project,"
    echo "   you might need to run initialization commands manually."
    echo "   Example: just init-repo"
    echo ""
    echo "   Use 'just --list' to see available project-specific tasks."
    echo "--------------------------------------------------"
    echo ""
    rustup default stable
  '';
}
