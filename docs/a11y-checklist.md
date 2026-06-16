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
