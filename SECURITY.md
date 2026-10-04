# Security policy

Relay OS is an experimental hobby operating system. It is not meant to
protect anything valuable yet, but memory-safety, privilege-separation and
boot-path bugs are still welcome reports.

## Reporting a vulnerability

Use a private
[GitHub security advisory](https://github.com/bin-inc/relay-os/security/advisories/new)
("Report a vulnerability" on the Security tab). Please do not open a public
issue for a vulnerability.

Include the version (`git describe` or the release tag), how you ran it
(QEMU or hardware) and the steps to reproduce.

## Supported versions

Only the latest release and `main` get fixes.
