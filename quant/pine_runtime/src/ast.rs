#[derive(Clone, Debug)]
pub(crate) enum Expr {
    Number(f64),
    String(String),
    Name(String),
    Unary {
        operator: String,
        value: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        operator: String,
        right: Box<Expr>,
    },
    Conditional {
        condition: Box<Expr>,
        yes: Box<Expr>,
        no: Box<Expr>,
    },
    Index {
        value: Box<Expr>,
        offset: Box<Expr>,
    },
    Call {
        name: String,
        arguments: Vec<Argument>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Argument {
    pub name: Option<String>,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub(crate) enum Statement {
    Assign {
        name: String,
        value: Expr,
        declaration: DeclarationMode,
    },
    Expression(Expr),
    If {
        condition: Expr,
        body: Vec<Statement>,
        otherwise: Vec<Statement>,
    },
    For {
        name: String,
        start: Expr,
        end: Expr,
        body: Vec<Statement>,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DeclarationMode {
    #[default]
    Ordinary,
    Var,
    Varip,
}

#[derive(Clone, Debug)]
pub(crate) struct Program {
    pub statements: Vec<Statement>,
    pub validation: crate::Validation,
    pub is_strategy: bool,
}
