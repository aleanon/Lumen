# Accessibility verification checklist (T4.3)

Lumen's single semantic tree drives platform accessibility through AccessKit
(`lumen_widgets::a11y`). The automated `AccessKit-tree diff` tests
(`cargo test -p lumen-widgets --test a11y`) verify the role/state mapping; this
checklist covers the **manual** screen-reader passes that automation can't.

## Role map (Lumen → AccessKit)

| Lumen role | AccessKit role | Lumen role | AccessKit role |
|---|---|---|---|
| Window | Window | MenuItem | MenuItem |
| Button | Button | Dialog | Dialog |
| Checkbox | CheckBox | Alert | Alert |
| Radio | RadioButton | Tooltip | Tooltip |
| Switch | Switch | Progress | ProgressIndicator |
| Slider | Slider | Group | Group |
| TextInput | TextInput | ScrollArea | ScrollView |
| Text | Label | Tree | Tree |
| Image | Image | TreeItem | TreeItem |
| Link | Link | ComboBox | ComboBox |
| List / ListItem | List / ListItem | Generic | GenericContainer |
| Table / Row / Cell | Table / Row / Cell | ColumnHeader | ColumnHeader |
| TabList / Tab / TabPanel | TabList / Tab / TabPanel | Menu | Menu |

State map: Checked/Unchecked/Mixed → `toggled`; Selected → `selected`;
Expanded/Collapsed → `expanded`; Disabled → `disabled`; Readonly → `read_only`;
Required → `required`; Busy → `busy`. Focused/Hovered/Pressed are runtime states
(focus rides on `TreeUpdate.focus`, not a node property).

## macOS — VoiceOver (Cmd-F5)
- [ ] Tab order matches visual order; each control announces role + label.
- [ ] Button: "Save, button"; activating with VO-Space fires the action.
- [ ] Checkbox/Switch announce checked/unchecked and update on toggle.
- [ ] Slider announces value and responds to VO arrow adjustment.
- [ ] Tree: items announce "expanded/collapsed"; expanding reveals children.
- [ ] DataGrid: row/column headers announced when navigating cells.
- [ ] TextInput: label + current value; typing is echoed.
- [ ] Dialog traps focus; Escape dismisses; Alert is announced immediately.

## Windows — NVDA
- [ ] Object navigation reaches every interactive control.
- [ ] Roles map sensibly (NVDA reads "check box", "tree view item", etc.).
- [ ] Toggled/expanded/selected states are spoken on change.
- [ ] Forms mode lets the user type into TextInput; browse mode reads content.
- [ ] Focus changes (tab, dialog open) are announced.

## Linux — Orca / AT-SPI (smoke)
- [ ] `accerciser` shows the Lumen tree with correct roles + states.

## WCAG 2.2 — automated vs manual (T7.4)
Automated in `lumen_widgets::wcag` + `audit` (run in CI over the semantic tree):
- [x] 1.4.3 Contrast (min): `contrast_ratio` / `meets_aa` (4.5:1 text, 3:1 large).
- [x] 2.5.5 Target size: `audit::audit_touch_targets` (≥44px).
- [x] 4.1.2 Name, Role, Value: `wcag::audit_names` flags unnamed interactives;
      role/state via the AccessKit map (`a11y`).

Manual / screen-reader CI (PENDING a mac+win+linux a11y runner): the VoiceOver /
NVDA / Orca passes above. The role/state map + the automated checks are the
machine-verifiable half of conformance.
