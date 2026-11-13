# zyt-bytes

The buffer one thread fills with bytes and another swaps out: `ByteSwap`.

## Scope

| item | what it does |
| --- | --- |
| `ByteSwap::with_size(bytes)` | a buffer of a size, allocated now; zero is no limit |
| `ByteSwap::with_capacity(bytes)` | a buffer with no limit and room for `bytes` allocated now |
| `set_size(bytes)`, `size()` | another size; the buffer in hand follows at once, the spare on the next swap |
| `fill(buffer, wait, read)` | waits for room, hands `read` as much of `buffer` as fits, appends all it read; `None` when no room came |
| `is_full()` | the buffer stands at its size |
| `push(&[u8])` | appends a chunk, all of it: a buffer with no limit |
| `take_into(&mut Vec<u8>)` | swaps everything waiting into the spare of the caller |
| `take_into_wait(&mut Vec<u8>, timeout)` | the same, after waiting for bytes |
| `wake()` | wakes both waits: a side that ends says so |

A size lowered while a read is under way is kept from the next read on: what
the read brought goes in whole, and the buffer stands over the new size by that
one read until it is taken.
| `len()`, `is_empty()`, `capacity()`, `clear()` | |

## Boundaries

It knows bytes and two threads, nothing of a port, a console or a script.
Payload goes through it and through nothing else: no queue, no channel.

## Errors

None. A poisoned lock reads as an empty buffer and a write that does nothing.
