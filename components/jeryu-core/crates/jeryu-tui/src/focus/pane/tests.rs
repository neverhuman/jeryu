use super::*;

#[test]
fn default_pane_belongs_to_its_tab() {
    for tab in ActiveTab::ALL {
        let pane = PaneId::default_for_tab(*tab);
        assert_eq!(
            pane.tab(),
            *tab,
            "default pane for {tab:?} must report that tab"
        );
    }
}

#[test]
fn every_tab_has_at_least_one_pane() {
    for tab in ActiveTab::ALL {
        assert!(!PaneId::panes_for_tab(*tab).is_empty());
    }
}

#[test]
fn fleet_bar_is_the_only_global_pane() {
    assert!(PaneId::FleetBar.is_global());
    assert!(!PaneId::ReposLens.is_global());
}

#[test]
fn pane_labels_are_non_empty() {
    for tab in ActiveTab::ALL {
        for pane in PaneId::panes_for_tab(*tab) {
            assert!(!pane.label().is_empty());
        }
    }
}
