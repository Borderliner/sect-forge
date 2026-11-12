use std::ops::Range;

use crate::game::TraitCategory;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TraitView {
    List,
    Grid,
}

impl TraitView {
    pub const ALL: [(Self, &'static str); 2] = [(Self::List, "List"), (Self::Grid, "Grid")];
}

pub(super) const TRAIT_CATEGORIES: [(TraitCategory, &str, u32); 7] = [
    (TraitCategory::Resolve, "Resolve", 0x85aa78),
    (TraitCategory::Cunning, "Cunning", 0x7aaac4),
    (TraitCategory::Instinct, "Instinct", 0xd58173),
    (TraitCategory::Authority, "Authority", 0xd6ad65),
    (TraitCategory::Expertise, "Expertise", 0xb19dc8),
    (TraitCategory::Good, "Good", super::JADE),
    (TraitCategory::Bad, "Bad", super::CINNABAR),
];

pub(super) const RARITIES: [(&str, &str, u32); 4] = [
    ("common", "Common", super::MUTED),
    ("refined", "Refined", 0xcd9e79),
    ("exceptional", "Exceptional", 0xaac3d1),
    ("unique", "Unique", super::GOLD),
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Catalog {
    Portraits,
    Traits,
    Equipment,
    Workshop,
}

impl Catalog {
    pub fn label(self) -> &'static str {
        match self {
            Self::Portraits => "portraits",
            Self::Traits => "traits",
            Self::Equipment => "items",
            Self::Workshop => "packs",
        }
    }

    pub fn page_size(self, view: TraitView) -> usize {
        match self {
            Self::Portraits => 24,
            Self::Traits if view == TraitView::List => 10,
            Self::Traits | Self::Equipment | Self::Workshop => 12,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct CatalogPage {
    pub index: usize,
    pub pages: usize,
    size: usize,
    length: usize,
}

impl CatalogPage {
    pub fn new(index: usize, size: usize, length: usize) -> Self {
        let pages = length.div_ceil(size);
        let index = index.min(pages.saturating_sub(1));
        Self {
            index,
            pages,
            size,
            length,
        }
    }

    pub fn range(self) -> Range<usize> {
        self.index * self.size..((self.index + 1) * self.size).min(self.length)
    }

    pub fn summary(self, label: &str) -> String {
        let range = self.range();
        if range.is_empty() {
            return format!("0 {label}");
        }
        format!(
            "{}–{} of {} {label}",
            range.start + 1,
            range.end,
            self.length
        )
    }

    // Keep the first and last page and a compact group around the current page.
    // A single missing page gets its own button instead of an ellipsis.
    pub fn buttons(self) -> Vec<Option<usize>> {
        if self.pages <= 7 {
            return (0..self.pages).map(Some).collect();
        }
        let start = self.index.saturating_sub(1).clamp(1, self.pages - 4);
        let end = start + 3;
        let mut buttons = vec![Some(0)];
        match start {
            1 => {}
            2 => buttons.push(Some(1)),
            _ => buttons.push(None),
        }
        buttons.extend((start..end).map(Some));
        match self.pages - end {
            1 => {}
            2 => buttons.push(Some(end)),
            _ => buttons.push(None),
        }
        buttons.push(Some(self.pages - 1));
        buttons
    }
}

#[cfg(test)]
mod tests {
    use super::CatalogPage;

    #[test]
    fn filtered_results_clamp_to_a_valid_page() {
        let empty = CatalogPage::new(7, 12, 0);
        assert_eq!(empty.range(), 0..0);
        let final_page = CatalogPage::new(8, 12, 26);
        assert_eq!(final_page.range(), 24..26);
        assert_eq!(final_page.buttons(), vec![Some(0), Some(1), Some(2)]);
    }

    #[test]
    fn large_catalogs_keep_current_and_endpoint_page_buttons() {
        for index in 0..100 {
            let buttons = CatalogPage::new(index, 10, 1000).buttons();
            assert_eq!(buttons.first(), Some(&Some(0)));
            assert_eq!(buttons.last(), Some(&Some(99)));
            assert!(buttons.contains(&Some(index)));
            assert!(buttons.len() <= 7);
        }
    }
}
