//! Choosing the root partition (spec §6.5): the Linux filesystem partition on
//! the disk we booted from, found by the boot partition's GUID from
//! `BootInfo`, with a fallback for firmware that does not report it.

use super::gpt::{ESP_TYPE, Gpt, GptPartition, Guid, LINUX_FS_TYPE};
use core::fmt;

/// The partition to mount as `/`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootChoice {
    /// Index into the slice given to choose_root.
    pub disk: usize,
    pub partition: GptPartition,
    /// No disk had the boot partition; this is the fallback of spec §6.5 (the caller logs a warning).
    pub fallback: bool,
}

/// Why no root partition was chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoRoot {
    /// No disk has a readable GPT.
    NoDisks,
    /// The boot disk was found but has no Linux filesystem partition.
    NoLinuxPartition,
    /// No disk has the boot partition, and none has exactly one ESP and one Linux partition.
    NoMatch,
}

impl fmt::Display for NoRoot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            NoRoot::NoDisks => "no disk with a GPT",
            NoRoot::NoLinuxPartition => "no Linux partition on the boot disk",
            NoRoot::NoMatch => "no disk with the boot partition",
        })
    }
}

/// The partitions of `gpt` with type `type_guid`, in entry order.
fn of_type(gpt: &Gpt, type_guid: Guid) -> impl Iterator<Item = &GptPartition> {
    gpt.partitions
        .iter()
        .filter(move |p| p.type_guid == type_guid)
}

/// Spec §6.5: the disk whose GPT has a partition with unique GUID `boot` (from BootInfo), and on
/// it the first Linux filesystem partition. Otherwise (no boot GUID, or no disk has it) the first disk
/// with exactly one ESP partition and exactly one Linux filesystem partition (other partition types
/// do not matter), with `fallback` set. Disks whose GPT could not be read are `None`.
///
/// A zero `boot` GUID is treated as none: it would only match corrupt entries.
/// If several disks have the boot partition (a stick copied with `dd`), the
/// first one wins.
pub fn choose_root(disks: &[Option<Gpt>], boot: Option<Guid>) -> Result<RootChoice, NoRoot> {
    let readable = || {
        disks
            .iter()
            .enumerate()
            .filter_map(|(i, d)| Some((i, d.as_ref()?)))
    };
    if readable().next().is_none() {
        return Err(NoRoot::NoDisks);
    }
    if let Some(boot) = boot.filter(|g| !g.is_zero()) {
        let boot_disk =
            readable().find(|(_, gpt)| gpt.partitions.iter().any(|p| p.unique_guid == boot));
        if let Some((disk, gpt)) = boot_disk {
            let linux = of_type(gpt, LINUX_FS_TYPE)
                .next()
                .ok_or(NoRoot::NoLinuxPartition)?;
            return Ok(RootChoice {
                disk,
                partition: linux.clone(),
                fallback: false,
            });
        }
    }
    readable()
        .find_map(|(disk, gpt)| {
            let mut linux = of_type(gpt, LINUX_FS_TYPE);
            match (linux.next(), linux.next(), of_type(gpt, ESP_TYPE).count()) {
                (Some(partition), None, 1) => Some(RootChoice {
                    disk,
                    partition: partition.clone(),
                    fallback: true,
                }),
                _ => None,
            }
        })
        .ok_or(NoRoot::NoMatch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;
    use alloc::vec::Vec;

    const OTHER_TYPE: Guid = Guid([0x42; 16]);

    /// A GUID numbered `n`, unique within a test.
    fn id(n: u8) -> Guid {
        Guid::from_fields(0x5245_4C41, 0x5900, 0x4000, [0x80, 0, 0, 0, 0, 0, 0, n])
    }

    /// A partition of `type_guid` whose unique GUID is `id(n)`.
    fn part(n: u8, type_guid: Guid) -> GptPartition {
        let first = n as u64 * 1000;
        GptPartition {
            number: n as u32,
            type_guid,
            unique_guid: id(n),
            first_lba: first,
            last_lba: first + 999,
        }
    }

    fn disk(partitions: Vec<GptPartition>) -> Option<Gpt> {
        Some(Gpt {
            disk_guid: id(0),
            partitions,
            used_backup: false,
        })
    }

    /// The usual stick: ESP `id(esp)` and Linux `id(esp + 1)`.
    fn stick(esp: u8) -> Option<Gpt> {
        disk(vec![part(esp, ESP_TYPE), part(esp + 1, LINUX_FS_TYPE)])
    }

    fn chosen(disk: usize, partition: GptPartition, fallback: bool) -> Result<RootChoice, NoRoot> {
        Ok(RootChoice {
            disk,
            partition,
            fallback,
        })
    }

    #[test]
    fn the_boot_esp_picks_its_disks_linux_partition() {
        // BootInfo carries the GUID of the ESP we booted from.
        let disks = [stick(1), stick(3)];
        assert_eq!(
            choose_root(&disks, Some(id(3))),
            chosen(1, part(4, LINUX_FS_TYPE), false)
        );
        assert_eq!(
            choose_root(&disks, Some(id(1))),
            chosen(0, part(2, LINUX_FS_TYPE), false)
        );
    }

    #[test]
    fn the_boot_guid_may_be_any_partition_of_the_disk() {
        let disks = [
            stick(1),
            disk(vec![part(3, OTHER_TYPE), part(4, LINUX_FS_TYPE)]),
        ];
        assert_eq!(
            choose_root(&disks, Some(id(3))),
            chosen(1, part(4, LINUX_FS_TYPE), false)
        );
    }

    #[test]
    fn the_first_linux_partition_of_the_boot_disk() {
        let disks = [disk(vec![
            part(1, OTHER_TYPE),
            part(2, ESP_TYPE),
            part(3, LINUX_FS_TYPE),
            part(4, LINUX_FS_TYPE),
        ])];
        assert_eq!(
            choose_root(&disks, Some(id(2))),
            chosen(0, part(3, LINUX_FS_TYPE), false)
        );
    }

    #[test]
    fn a_boot_disk_without_linux_partition_is_an_error() {
        // Even though the second disk would do for the fallback.
        let disks = [disk(vec![part(1, ESP_TYPE), part(2, OTHER_TYPE)]), stick(3)];
        assert_eq!(
            choose_root(&disks, Some(id(1))),
            Err(NoRoot::NoLinuxPartition)
        );
    }

    #[test]
    fn the_first_disk_with_the_boot_guid_wins() {
        // A stick copied with dd has the same GUIDs as the original.
        let disks = [stick(1), stick(1)];
        assert_eq!(
            choose_root(&disks, Some(id(1))),
            chosen(0, part(2, LINUX_FS_TYPE), false)
        );
    }

    /// Disks that do not qualify for the fallback, then one that does.
    fn fallback_disks() -> Vec<Option<Gpt>> {
        vec![
            None,
            disk(vec![]),
            disk(vec![
                part(1, ESP_TYPE),
                part(2, LINUX_FS_TYPE),
                part(3, LINUX_FS_TYPE),
            ]),
            disk(vec![
                part(4, ESP_TYPE),
                part(5, ESP_TYPE),
                part(6, LINUX_FS_TYPE),
            ]),
            disk(vec![part(7, LINUX_FS_TYPE)]),
            disk(vec![part(8, ESP_TYPE)]),
            disk(vec![
                part(9, OTHER_TYPE),
                part(10, LINUX_FS_TYPE),
                part(11, ESP_TYPE),
            ]),
            stick(12),
        ]
    }

    #[test]
    fn without_a_boot_guid_the_first_disk_with_one_esp_and_one_linux_partition() {
        // Other partition types do not matter, nor their order.
        assert_eq!(
            choose_root(&fallback_disks(), None),
            chosen(6, part(10, LINUX_FS_TYPE), true)
        );
    }

    #[test]
    fn an_unknown_boot_guid_falls_back() {
        assert_eq!(
            choose_root(&fallback_disks(), Some(id(99))),
            chosen(6, part(10, LINUX_FS_TYPE), true)
        );
        assert_eq!(
            choose_root(&[stick(1)], Some(id(99))),
            chosen(0, part(2, LINUX_FS_TYPE), true)
        );
    }

    #[test]
    fn a_zero_boot_guid_matches_nothing() {
        // A corrupt entry may have a zero unique GUID; it is not the boot one.
        let mut odd = part(1, LINUX_FS_TYPE);
        odd.unique_guid = Guid([0; 16]);
        let disks = [disk(vec![odd]), stick(2)];
        assert_eq!(
            choose_root(&disks, Some(Guid([0; 16]))),
            chosen(1, part(3, LINUX_FS_TYPE), true)
        );
    }

    #[test]
    fn nothing_qualifies() {
        let mut disks = fallback_disks();
        disks.truncate(6);
        assert_eq!(choose_root(&disks, None), Err(NoRoot::NoMatch));
        assert_eq!(choose_root(&disks, Some(id(99))), Err(NoRoot::NoMatch));
    }

    #[test]
    fn no_readable_gpt_is_no_disks() {
        assert_eq!(choose_root(&[], None), Err(NoRoot::NoDisks));
        assert_eq!(choose_root(&[], Some(id(1))), Err(NoRoot::NoDisks));
        assert_eq!(
            choose_root(&[None, None], Some(id(1))),
            Err(NoRoot::NoDisks)
        );
    }

    #[test]
    fn errors_are_short() {
        assert_eq!(NoRoot::NoDisks.to_string(), "no disk with a GPT");
        assert_eq!(
            NoRoot::NoLinuxPartition.to_string(),
            "no Linux partition on the boot disk"
        );
        assert_eq!(
            NoRoot::NoMatch.to_string(),
            "no disk with the boot partition"
        );
    }
}
