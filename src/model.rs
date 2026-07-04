#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueState {
    Open,
    Closed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub number: u32,
    pub title: String,
    pub body: String,
    pub labels: Vec<Label>,
    pub state: IssueState,
    pub url: String,
}
