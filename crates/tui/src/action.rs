#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    MoveDown,
    MoveUp,
    JumpTop,
    JumpBottom,
    PageDown,
    PageUp,
    ViewportTop,
    ViewportMiddle,
    ViewportBottom,
    CenterCurrent,
    SwitchPane,
    OpenSelected,
    Back,
    QuitView,
}
