# zyt-bytes

The buffer one thread fills with bytes and another swaps out: `ByteSwap`.

## Scope

| item | what it does |
| --- | --- |
| `ByteSwap::with_size(bytes)` | a buffer of a size, allocated now; zero is no limit |
| `ByteSwap::with_capacity(bytes)` | a buffer with no limit and room for `bytes` allocated now |
| `set_size(bytes)`, `size()` | another size; the buffer in hand follows at once, the spare on the next swap |
| `room()`, `is_full()`, `wait_for_room(timeout)` | what the side that fills reads, and the wait while there is no room |
| `push(&[u8])` | appends what fits and answers how much |
| `take_into(&mut Vec<u8>)` | swaps everything waiting into the spare of the caller |
| `take_into_wait(&mut Vec<u8>, timeout)` | the same, after waiting for bytes |
| `wake()` | wakes both waits: a side that ends says so |
| `len()`, `is_empty()`, `capacity()`, `clear()` | |

## Boundaries

It knows bytes and two threads, nothing of a port, a console or a script.
Payload goes through it and through nothing else: no queue, no channel.

## Errors

None. A poisoned lock reads as an empty buffer and a write that does nothing.
