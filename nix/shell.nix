{
  mkShell,
  stdenvAdapters,
  cargo-watch,
  privchat,
}:
mkShell.override (old: {
  stdenv = stdenvAdapters.useMoldLinker old.stdenv;
}) {
  inputsFrom = [
    privchat
  ];

  packages = [
    cargo-watch
  ];
}
