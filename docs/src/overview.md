Relics Overview
=======================

Welcome to the Relics Protocol (⟁), an attempt to enable AMM (automated
market making) directly on the Bitcoin L1.

The Case for Relics
-------------------

On EVM chains and Solana, Automated Market Making (AMM) protocols
provide a seamless way to trade fungible tokens on-chain. However, the
two major fungible token standards on Bitcoin, BRC-20 and Runes, don't
have in-protocol support for AMM trading or didn't have from the
beginning, what makes bootstrapping liquidity very difficult. They trade
similar to NFTs, so that each trade requires a buyer to actively accept
a seller's listing, and vice-versa.

This may be sufficient for large cap tokens which are listed on
centralized exchanges and have market makers, but for medium and smaller
tokens it is hard to bootstrap reasonable liquidity and trading them
comes with meaningful friction.

Relics' solution approach
-------------------------

The relics protocol is an attempt to solve this challenge and to enable
true AMM directly on Bitcoin L1 by creating a native liquidity
metaprotocol for tokens and NFTs.

1. Relics: Fungible tokens on the relics protocol.
2. $MBTC*: The base token for liquidity pools.
3. Bonding curve mints: Capture more "value" in liquidity vs. gas fees.
4. Pools: Once a Relic mints out, an on-chain swap pool MBTC/<RELIC> is
   created.
5. Tradable tickers: Tickers like e.g. DOG•TO•THE•MOON are inscriptions
   itself.

Relics vs Runes
---------------

[Runes](https://docs.ordinals.com/runes.html) is a great and simple
protocol for fungible tokens on Bitcoin,
similar to
how [ERC-20](https://ethereum.org/en/developers/docs/standards/tokens/erc-20/)
is a good token standard for fungible tokens on
Ethereum. Relics is more like a mix between a token standard and some
application logic.

*$MBTC, a fair trade-off?
-------------------------

MBTC stands for Meme Bitcoin. The name was chosen humorously because
MBTC isn't real Bitcoin. Think of it more as a fungible collectible.
Some may argue that using real BTC to bootstrap liquidity would be
preferable, but that isn't possible on Bitcoin L1 right now. Let's see
if it becomes feasible in the future once new OP codes (
like [OP_CAT](https://bitcoinops.org/en/topics/op_cat/)) are
merged. If so, that would be exciting. Until then, we're happy to accept
the trade-off.

Philosophy
----------

A "memecoin" is a cryptocurrency inspired by internet memes or cultural
trends, typically lacking any utility or intrinsic value (on purpose).
Memecoins on Solana have a short life expectancy: most launch on
pump.fun, get traded by "degens" for a few hours, and then die. On
Bitcoin, we believe memecoins should be viewed more as collectibles; if
not, there's no point in having them on Bitcoin. Solana is faster and
cheaper, but Bitcoin is forever. The Relics protocol wasn't created for
coins that last only a few blocks — it was designed to host fungible
collectibles for eternity.

Authors
-------

- [Curvetoshi](https://github.com/curvetoshi)

Links
-----

- [GitHub](https://github.com/relicsprotocol/relics-ord/)
- [Relics Protocol X](https://x.com/relics_btc)