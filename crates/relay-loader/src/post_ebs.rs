pub(crate) trait FinalMap {
    fn len(&self) -> usize;
    fn sort(&mut self);
}

pub(crate) fn sort_final_map(map: &mut impl FinalMap, maximum_regions: usize) -> Result<(), ()> {
    if map.len() > maximum_regions {
        return Err(());
    }
    map.sort();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CountingMap {
        len: usize,
        sorts: usize,
    }

    impl FinalMap for CountingMap {
        fn len(&self) -> usize {
            self.len
        }

        fn sort(&mut self) {
            self.sorts += 1;
        }
    }

    #[test]
    fn capacity_guard_rejects_the_map_without_sorting() {
        let mut map = CountingMap { len: 513, sorts: 0 };

        assert_eq!(sort_final_map(&mut map, 512), Err(()));
        assert_eq!(map.sorts, 0);
    }

    #[test]
    fn accepted_map_is_sorted_exactly_once() {
        let mut map = CountingMap { len: 512, sorts: 0 };

        assert_eq!(sort_final_map(&mut map, 512), Ok(()));
        assert_eq!(map.sorts, 1);
    }
}
