# Send Plugins send audio through one shared-memory table

Send Plugins pass audio to the app through one shared-memory segment: a versioned table (`dasmeter.v1`, `Local\dasmeter.v1` on Windows) with 64 fixed slots. Each slot holds one Send Plugin's details (ID, name, colour, mono flag, sample rate, host PID, heartbeat) and a ring of 16,384 stereo frames. The audio thread only copies samples and bumps a 64-bit frame counter. It never waits, never makes syscalls, and overwrites the oldest audio, because a Meter only wants the newest. The app sets a per-slot "listened to" flag, and a Send Plugin writes audio only while it's set.

We rejected sockets, pipes and Mach messages: each send is a syscall, so they would need a second thread and ring inside the plugin. We rejected one segment per Send Plugin: on macOS a crashed DAW leaves its segment behind, and a segment's size can't change once set. One table sidesteps both. MiniMetersServer uses a single fixed-name segment where the newest instance wins, which can't serve one Meter per Send Plugin.

The layout version is part of the name. A new layout means a new name (`v2`), and the app reads both for at least one major version, so older Send Plugins in saved projects keep working. The segment is readable and writable by the same user only.

**Open risk:** Logic Pro runs AUs in `AUHostingService`, whose sandbox isn't documented. If shared memory is blocked there, the AU declares `mach-lookup.global-name` in its `resourceUsage`, and the app offers a Mach service that hands over the same memory. The table and the audio path stay the same. This is tested in [Test the Send Plugin's shared memory inside Logic Pro](https://github.com/daswerk/Das-Meter/issues/16).

Decided in [Choose the Send Plugin transport](https://github.com/daswerk/Das-Meter/issues/12). Research: `docs/research/send-plugin-transport.md` on branch `research/send-plugin-transport`.
