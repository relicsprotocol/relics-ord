Relics
=====

Relics allow Bitcoin transactions to seal, enshrine, mint, and transfer
Bitcoin Memecoins.

Whereas every inscription is unique, every unit of a relic is the same.
They are interchangeable tokens, fit for a variety of purposes.

Keepsakes
---------

Relic protocol messages, called keepsakes, are stored in Bitcoin
transaction outputs.

A keepsake output's script pubkey begins with an `OP_RETURN`, followed
by `OP_15`, followed by zero or more data pushes. These data pushes are
concatenated and decoded into a sequence of 128-bit integers, and
finally parsed into a keepsake.

A transaction may have at most one keepsake.

A keepsake may seal a new relic, enshrine a sealed relic, mint an
existing relic, and transfer relics from a transaction's inputs to its
outputs.

A transaction output may hold balances of any number of relics.

Relics are identified by IDs, which consist of the block in which a
relic was enshrined and the index of the enshrining transaction
within that block, represented in text as `BLOCK:TX`. For example, the
ID of the relic etched in the 20th transaction of the 500th block is
`500:20`.

Sealing
-------

Relics come into existence through sealing. Sealing reserves a relic but
does not launch it yet. Depending on the length of the relic’s name,
MBTC must be burned to become eligible for sealing.

| length | MBTC   | 
|--------|--------|
| 13+    | 1      |
| 7-12   | 10     |
| 4-6    | 500    |
| 3      | 2100   |
| 2      | 21000  |
| 1      | 210000 |

### Sealing Inscriptions

Each sealing is also an inscription. The content of the inscription is
used as the thumbnail. The metadata field RELIC contains the name of the
relic. This means sealings can be traded on ordinal marketplaces.

### Frontrunning

Sealings can be frontrunned by design. If your sealing is frontrunned,
the MBTC spent will be refunded in the same transaction.

### Name

Names consist of the letters A through Z and are between one and
twenty-six letters long. For example `UNCOMMONRELICS` is a relic name.

Names may contain spacers, represented as bullets, to aid readability.
`UNCOMMONRELICS` might be sealed as `UNCOMMON•RELICS`.

The uniqueness of a name does not depend on spacers. Thus, a relic
may not be sealed with the same sequence of letters as an existing
relic, even if it has different spacers.

Spacers can only be placed between two letters. Finally, spacers do not
count towards the letter count.

Enshrining
----------

Sealings are turned into mintable relics through enshrining. Only the
owner of a sealing inscription may enshrine the relic. There is no
additional fee for enshrining.

### Divisibility

A relic's divisibility is how finely it may be divided into its atomic
units. Divisibility is expressed as the number of digits permissible
after the
decimal point in an amount of relics. All relics have the same
divisibility: 8

### Symbol

A relic's currency symbol is a single Unicode code point, for example
`$`, `⧉`, or `🧿`.

If a relic does not have a symbol, the generic currency sign `¤`, also
called a scarab, should be used.

### Mint Terms

A relic may have an open mint, allowing anyone to create and
allocate units of that relic for themselves. An open mint is subject to
terms, which are set upon enshrining.

A mint is open while all terms of the mint are satisfied, and closed
when any of them are not.

#### Cap

The number of times a relic may be minted is its cap. A mint is closed
once the cap is reached.

#### Block Cap

Optional maximum number of mints per block.

#### Tx Cap

Optional maximum number of mints per tx (relics supports up to 255
mints per tx by default).

#### Amount

Each mint transaction creates a fixed amount of new units of a relic.

#### Max Unmints

If set, mints can be unminted. Unminting returns the spent MBTC in
exchange for the minted relics. The unminted relics can then be minted
again by someone else. If the max unmints is set to 100, only 100 mints
can be unminted before the feature is disabled.

#### Price

There are two ways to set the mint price:

**Fixed:** Each mint costs a fixed amount of MBTC.

**Formula:** Define three values: `a`, `b`, and `c`. The price is
then calculated as `price(x) = a - (b / (c + x))`, where x is the number
of mints so far.

#### Seed

The number of tokens that are added to a liquidity pool, together with
the raised MBTC, once the mint concludes.

Example: If 10,000 relics are minted for each 1 MBTC and the seed is set
to 10,000, then 10,000 MBTC and 10,000 relics are deposited into the
pool.

Minting
-------

While a relic's mint is open, anyone may create a mint transaction that
creates a fixed amount of new units of that relic, subject to the terms
of the mint.

Swapping
--------

After the mint concludes, relics can be swapped using the newly created
liquidity pool. The pool provides full-range liquidity via
an [AMM](https://chain.link/education-hub/what-is-an-automated-market-maker-amm).

Transferring
------------

When transaction inputs contain relics, or new relics are created by a
mint, those relics are transferred to that transaction's outputs. A
transaction's keepsake may change how input relics transfer to outputs.

### Transfers

A keepsake may contain any number of transfers. Transfers consist of a
relic ID, an amount, and an output number. Transfers are processed in
order, allocating unallocated relics to outputs.

### Pointer

After all transfers are processed, remaining unallocated relics are
transferred to the transaction's first non-`OP_RETURN` output. A
keepsake optionally contain a pointer that specifies an alternative
default output.

### Burning

Relics may be burned by transferring them to an `OP_RETURN` output with
a transfer or pointer.

Cenotaphs
---------

Keepsakes may be malformed for a number of reasons, including
non-pushdata opcodes in the keepsake `OP_RETURN`, invalid varints, or
unrecognized keepsake fields.

Malformed keepsakes are
termed [cenotaphs](https://en.wikipedia.org/wiki/Cenotaph).

Relics input to a transaction with a cenotaph are burned. Relics etched
in a transaction with a cenotaph are set as unmintable. Mints in a
transaction with a cenotaph count towards the mint cap, but the minted
relics are burned.

Cenotaphs are an upgrade mechanism, allowing keepsakes to be given new
semantics that change how relics are created and transferred, while not
misleading unupgraded clients as to the location of those relics, as
unupgraded clients will see those relics as having been burned.
