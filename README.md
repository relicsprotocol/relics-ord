`relics-ord`
============

**⚠️⚠️⚠️ IMPORTANT ⚠️⚠️⚠️**

1. Read the [disclaimer](https://docs.relicsprotocol.com/relics)
   before "using" the protocol.
2. The documentation is currently being migrated from GitBook, so some
   links may be broken in the meantime.
3. Indexing may still contain bugs that could lead to loss of funds, so
   review the code carefully before relying on it.

This is an index, block explorer, and command-line wallet. It is
experimental software with no warranty. See [LICENSE](LICENSE) for more
details.

Relics is an indexing of fungible tokens on UTXOs. The Relics
codebase is a fork of the [ord](https://github.com/ordinals/ord).

See [the docs](https://docs.relicsprotocol.com) for documentation and
guides.

Wallet
------

`ord` relies on Bitcoin Core for private key management and
transaction signing. This has a number of implications that you must
understand in order to use `ord` wallet commands safely:

- Bitcoin Core is not aware of inscriptions and does not perform sat
  control. Using `bitcoin-cli` commands and RPC calls with `ord` wallets
  may lead to loss of inscriptions.

- `ord wallet` commands automatically load the `ord` wallet given by the
  `--name` option, which defaults to 'ord'. Keep in mind that after
  running an `ord wallet` command, an `ord` wallet may be loaded.

- Because `ord` has access to your Bitcoin Core wallets, `ord` should
  not be used with wallets that contain a material amount of funds. Keep
  ordinal and cardinal wallets segregated.

Installation
------------

`ord` is written in Rust and can be built from
[source](https://github.com/relicsprotocol/relics-ord). Pre-built
binaries are available on the [releases page](https://github.
com/relicsprotocol/relics-ord/releases).

You can install the latest pre-built binary from the command line with:

```sh
curl --proto '=https' --tlsv1.2 -fsLS https://relicsprotocol.com/install.sh | bash -s
```

Once `ord` is installed, you should be able to run `ord 
--version` on the command line.

Building
--------

On Linux, `ord` requires `libssl-dev` when building from source.

On Debian-derived Linux distributions, including Ubuntu:

```
sudo apt-get install pkg-config libssl-dev build-essential
```

On Red Hat-derived Linux distributions:

```
yum install -y pkgconfig openssl-devel
yum groupinstall "Development Tools"
```

You'll also need Rust:

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Clone the `ord` repo:

```
git clone https://github.com/relicsprotocol/relics-ord.git
cd relics-ord
```

To build a specific version of `ord`, first checkout that
version:

```
git checkout <VERSION>
```

And finally to actually build `ord`:

```
cargo build --release
```

Once built, the `ord` binary can be found at `./target/release/ord`.

`ord` requires `rustc` version 1.86.0 or later. Run `rustc --version` to
ensure you have this version. Run `rustup update` to get the latest
stable release.

### Docker

A Docker image can be built with:

```
docker build -t ordinals/ord .
```

### Debian Package

To build a `.deb` package:

```
cargo install cargo-deb
cargo deb
```

Contributing
------------

We strongly recommend installing [just](https://github.com/casey/just)
to make running the tests easier. To run our CI test suite you would do:

```
just ci
```

This corresponds to the commands:

```
cargo fmt -- --check
cargo test --all
cargo test --all -- --ignored
```

Have a look at the [justfile](justfile) to see some more helpful recipes
(commands). Here are a couple more good ones:

```
just fmt
just fuzz
just doc
just watch ltest --all
```

If the tests are failing or hanging, you might need to increase the
maximum number of open files by running `ulimit -n 1024` in your shell
before you run the tests, or in your shell configuration.

Syncing
-------

`ord` requires a synced `bitcoind` node with `-txindex` to build the
index of satoshi locations. `ord` communicates with `bitcoind` via RPC.

If `bitcoind` is run locally by the same user, without additional
configuration, `ord` should find it automatically by reading the
`.cookie` file from `bitcoind`'s datadir, and connecting using the
default RPC port.

If `bitcoind` is not on mainnet, is not run by the same user, has a
non-default datadir, or a non-default port, you'll need to pass
additional flags to `ord`. See `ord --help` for details.

`bitcoind` RPC Authentication
-----------------------------

`ord` makes RPC calls to `bitcoind`, which usually requires a username
and password.

By default, `ord` looks a username and password in the cookie file
created by `bitcoind`.

The cookie file path can be configured using `--cookie-file`:

```
ord --cookie-file /path/to/cookie/file server
```

Alternatively, `ord` can be supplied with a username and password on the
command line:

```
ord --bitcoin-rpc-username foo --bitcoin-rpc-password bar server
```

Using environment variables:

```
export ORD_BITCOIN_RPC_USERNAME=foo
export ORD_BITCOIN_RPC_PASSWORD=bar
ord server
```

Or in the config file:

```yaml
bitcoin_rpc_username: foo
bitcoin_rpc_password: bar
```

Light Mode
----------

By default, the indexer will use a large amount of data because it
indexes all inscriptions in existence. If you only want to index Relics,
we recommend running the indexer with `--index-relics 
--store-relics-only`.

Logging
--------

`ord` uses [env_logger](https://docs.
rs/env_logger/latest/env_logger/). Set the `RUST_LOG` environment
variable in order to turn on logging. For example, run the server and
show `info`-level log messages and above:

```
$ RUST_LOG=info cargo run server
```

Set the `RUST_BACKTRACE` environment variable in order to turn on full
rust backtrace. For example, run the server and turn on debugging and
full
backtrace:

```
$ RUST_BACKTRACE=1 RUST_LOG=debug ord server
```

New Releases
------------

Release commit messages use the following template:

```
Release x.y.z

- Bump version: x.y.z → x.y.z
- Update changelog
- Update changelog contributor credits
- Update dependencies
```