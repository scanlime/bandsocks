🅱️🧦 bandsocks
================

just a funky experimental sandbox that's designed to work inside unprivileged docker

Takes inspiration from gaol, User Mode Linux, gvisor, chromium, and podman. The goal is to add an extra level of isolation to compute workloads we run as non-root within containers which are already somewhat locked down. This means that most high-powered kernel features like KVM and even user namespaces are off the table. The approach this project uses is based on seccomp to restrict system calls, and an emulated filesystem.

The intended API for this package is fairly high-level:

- download/unpack docker images into an emulated filesystem
- attach streams to emulated files for I/O
- run commands in the container
