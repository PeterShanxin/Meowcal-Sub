# ADR-0005: Client-partitioned Core storage with a shared model

**Date:** 2026-09-25  
**Status:** Accepted  
**Decision owners:** Meowcal Sub maintainers  
**Related:** [#252](https://github.com/PeterShanxin/Meowcal-Sub/issues/252),
[Meowcal-Sub-2#112](https://github.com/PeterShanxin/Meowcal-Sub-2/issues/112),
[ADR-0004](0004-versioned-meowcal-core.md)

This decision replaces the storage layout and retention rules in ADR-0004's
"Compatibility and storage" section. The rest of ADR-0004 remains in force.

## Context

ADR-0004 partitioned Core storage as
`%LOCALAPPDATA%/Meowcal/Core/<profile>/<version>/<architecture>` and retained
every version for rollback. Each Core release therefore installed another copy
of the 1.1 GB HY-MT model, and nothing removed the old ones. On the
maintainer's ARM64 machine six partitions held 6.7 GB, although the model hash
has not changed across Core 0.1.0 to 0.1.3.

Sub 1 and Sub 2 pin different Core versions, so neither can tell which shared
partitions the other still uses. For the same reason an uninstaller cannot
remove the engine without risking the other application's installation, and
the directory survived Sub 1's "Delete the application data" option.

## Decision

Storage is partitioned by client first:
`<base>/<client>/<profile>/<version>/<architecture>`, where `client` is the
`hello` client (`sub1` or `sub2`). Each application owns its client directory.

An install imports a verified runtime archive or model from any other Core
partition of the same profile and architecture as an NTFS hard link. A volume
that cannot link falls back to a verified copy. Legacy application roots are
still copied, because the application that owns them may keep changing its own
files.

Once its engine is ready, Core reclaims space in the background:

- it removes its own client's older versions, keeping the newest one besides
  the running version for rollback;
- it replaces every remaining partition's model that matches its manifest hash
  with a hard link to its own, including other clients' partitions and the
  unpartitioned layout Core 0.1.0 to 0.1.3 wrote.

A partition another process holds a lease on is skipped and retried on a later
start. An application's uninstaller removes its own client directory when the
user asks it to delete application data. It removes the unpartitioned layout
only when the other application has never run on the machine.

## Consequences

Identical models occupy disk space once per profile, however many versions and
applications use them. An upgrade no longer copies 1.1 GB, and removing any one
partition frees nothing that another still links.

Hard links share bytes as well as space. A model corrupted in place is corrupt
in every partition that links it; verification still detects that, and repair
downloads a fresh file into the partition being repaired. Links need NTFS; on
another file system Core copies, as before.

Rolling back to a version whose partition was removed re-imports the model
from a remaining partition as a link rather than a download. Core releases
before the client level read only the unpartitioned layout, which Core never
removes, so rolling back to them finds their own partition.

Custom storage bases chosen in Sub 1's setup are not known to the uninstaller
and are not removed by it.

## Alternatives considered

- **Keep a fixed number of old versions, or remove versions after a period of
  use.** Every retained copy of an unchanged model is wasted space, and neither
  rule tells an application which shared partitions the other still pins.
- **One unversioned directory that each release overwrites.** Sub 1 and Sub 2
  run different Core versions at the same time, and a running engine's files
  cannot be replaced.
- **A content-addressed model store beside the partitions.** It removes the
  duplication too, but changes the install layout every Core release reads,
  and still needs reference tracking to know when a model can be deleted. Hard
  links get that reference count from the file system.

## Verification and follow-up

Contract tests cover the client layout, linked import, the copy-only disk
requirement, stale-version removal, lease skipping, and hash-matched sharing.
The uninstall hook's decisions need a Windows uninstall with and without Sub 2
data present. Sub 2 adopts the layout when its Core pin moves to a release that
carries it
([Meowcal-Sub-2#112](https://github.com/PeterShanxin/Meowcal-Sub-2/issues/112)).
