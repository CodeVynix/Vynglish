# Vynglish

Vynglish is a programming language that reads like English. It has an interactive prompt, a way to define your own commands with `teach`, and modules for math, text, lists, randomness and time.

```
print "hello"
print hello
set name to "Barkha"
print "Hello, Barkha"
print name
```

## Install and run

Build the binary from source (you need [Rust](https://rustup.rs)):

```
cargo build --release
```

The program is `target\release\vyn.exe` (or `target\debug\vyn.exe` after `cargo build`). To put `vyn` on your PATH, run `cargo install --path .` instead.

Run the interactive prompt:

```
vyn
```

Run a script file:

```
vyn program.vyn
```

## Documentation

The command reference is in [docs.md](docs.md).