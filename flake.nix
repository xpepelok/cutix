{
  description = "cutix — video editor that runs on your own machine";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rust = pkgs.rust-bin.stable.latest.default;

        runtime = with pkgs; [
          alsa-lib
          fontconfig
          freetype
          libxkbcommon
          onnxruntime
          openssl
          vulkan-loader
          wayland
          xorg.libX11
          xorg.libXcursor
          xorg.libXi
          xorg.libXrandr
          xorg.libxcb
        ];

        nativeBuildInputs = with pkgs; [
          cmake
          makeWrapper
          patchelf
          pkg-config
          python3
          rust
        ];

        libraryPath = pkgs.lib.makeLibraryPath runtime;
        onnx = "${pkgs.onnxruntime}/lib/libonnxruntime.so";
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "cutix";
          version = "0.1.0";
          src = ./.;

          cargoLock.lockFile = ./Cargo.lock;

          buildInputs = runtime;
          inherit nativeBuildInputs;

          cargoBuildFlags = [ "-p" "cutix" ];
          doCheck = false;

          ORT_DYLIB_PATH = onnx;

          postFixup = ''
            patchelf --set-rpath "${libraryPath}" $out/bin/cutix
            wrapProgram $out/bin/cutix \
              --set ORT_DYLIB_PATH "${onnx}" \
              --prefix LD_LIBRARY_PATH : "${libraryPath}"
          '';

          meta = with pkgs.lib; {
            description = "Video editor that runs on your own machine";
            homepage = "https://github.com/xpepelok/cutix";
            license = licenses.mit;
            mainProgram = "cutix";
            platforms = platforms.linux;
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = runtime;
          inherit nativeBuildInputs;

          LD_LIBRARY_PATH = libraryPath;
          ORT_DYLIB_PATH = onnx;
          RUST_SRC_PATH = "${rust}/lib/rustlib/src/rust/library";
        };
      });
}
