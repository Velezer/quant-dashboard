use crate::{
    ast::{Argument, DeclarationMode, Expr, Program, Statement},
    lexer::{self, Kind, Token},
    Diagnostic, Validation,
};
use std::collections::HashSet;

pub(crate) fn parse(source: &str) -> Result<Program, Vec<Diagnostic>> {
    if source.len() > crate::runtime::MAX_SOURCE_BYTES {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Pine source exceeds the 24 KB limit.",
        )]);
    }
    let version = source.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix("// @version=")
            .or_else(|| trimmed.strip_prefix("//@version="))?
            .parse::<u8>()
            .ok()
    });
    if version != Some(6) {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Add `//@version=6`; only Pine version 6 is accepted.",
        )]);
    }
    let tokens = lexer::lex(source)?;
    let mut parser = Parser {
        tokens,
        cursor: 0,
        expression_depth: 0,
        diagnostics: Vec::new(),
        indicator_seen: false,
        overlay: false,
        name: String::new(),
        strategy_seen: false,
        declaration_count: 0,
        visual_output_seen: false,
    };
    let mut statements = Vec::new();
    while !parser.is(&Kind::End) {
        if parser.take(&Kind::Newline) {
            continue;
        }
        if parser.take(&Kind::Dedent) {
            continue;
        }
        if let Some(statement) = parser.statement() {
            statements.push(statement);
        } else {
            parser.recover_line();
        }
        if statements.len() > crate::runtime::MAX_STATEMENTS {
            parser.error_here("Pine source has too many statements.");
            break;
        }
    }
    if parser.declaration_count != 1 {
        parser.diagnostics.push(Diagnostic::error(
            1,
            1,
            format!(
                "CE10243: A script must contain exactly one declaration statement (`indicator()`, `strategy()`, or `library()`); found {}.",
                parser.declaration_count
            ),
        ));
    }
    if parser.indicator_seen && !parser.visual_output_seen {
        parser.diagnostics.push(Diagnostic::error(
            1,
            1,
            "CE10246: An indicator must contain at least one plot*(), barcolor(), bgcolor(), hline(), alertcondition(), or drawing call (line, label, box, table, or polyline).",
        ));
    }
    let mut declared = HashSet::new();
    collect_declared_names(&statements, &mut declared);
    validate_statements(&mut parser, &statements, &declared);
    if !parser.diagnostics.is_empty() {
        return Err(parser.diagnostics);
    }
    let validation = Validation { valid: true, script_type: if parser.strategy_seen { "strategy" } else { "indicator" }.to_string(), version: 6, name: parser.name, overlay: parser.overlay, approximations: vec![
        "Pine input controls use their declared defaults; interactive Pine settings are not yet editable.".to_string(),
        "Only the supported statement and built-in set is executable; unsupported constructs are rejected with source locations.".to_string(),
        "Realtime is unavailable. request.security() can use only confirmed 15m, 4h, or 1d history for the selected app instrument.".to_string(),
    ], diagnostics: Vec::new() };
    Ok(Program {
        statements,
        validation,
        is_strategy: parser.strategy_seen,
    })
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
    expression_depth: usize,
    diagnostics: Vec<Diagnostic>,
    indicator_seen: bool,
    overlay: bool,
    name: String,
    strategy_seen: bool,
    declaration_count: usize,
    visual_output_seen: bool,
}

impl Parser {
    fn statement(&mut self) -> Option<Statement> {
        let declaration = if self.take_identifier("var") {
            DeclarationMode::Var
        } else if self.take_identifier("varip") {
            DeclarationMode::Varip
        } else {
            DeclarationMode::Ordinary
        };
        if self.peek_identifier("const") {
            self.cursor += 1;
        }
        if ["while", "switch", "type", "method", "export", "import"]
            .iter()
            .any(|keyword| self.peek_identifier(keyword))
        {
            let token = self.current().clone();
            let keyword = match token.kind {
                Kind::Identifier(value) => value,
                _ => String::new(),
            };
            self.diagnostics.push(Diagnostic::error(
                token.line,
                token.column,
                format!("`{keyword}` statements are not supported by this runtime yet."),
            ));
            return None;
        }
        if self.take_identifier("if") {
            let condition = self.expression(0)?;
            self.inspect_expression(&condition);
            let body = self.block()?;
            let otherwise = if self.take_identifier("else") {
                self.block()?
            } else {
                Vec::new()
            };
            return Some(Statement::If {
                condition,
                body,
                otherwise,
            });
        }
        if self.take_identifier("for") {
            let Some(name) = self.current_identifier().map(str::to_string) else {
                self.error_here("Expected a loop variable after `for`.");
                return None;
            };
            self.cursor += 1;
            if !self.take_operator("=") {
                self.error_here("Expected `=` after the loop variable.");
                return None;
            }
            let start = self.expression(0)?;
            if !self.take_identifier("to") {
                self.error_here("Expected `to` in the bounded `for` loop.");
                return None;
            }
            let end = self.expression(0)?;
            self.inspect_expression(&start);
            self.inspect_expression(&end);
            return Some(Statement::For {
                name,
                start,
                end,
                body: self.block()?,
            });
        }
        let start = self.cursor;
        let typed_name = if self.current_identifier().is_some()
            && self.next_identifier().is_some()
            && self.operator_after_next("=")
        {
            self.cursor += 1;
            self.current_identifier().map(str::to_string)
        } else {
            None
        };
        if let Some(name) = typed_name {
            self.cursor += 1;
            self.cursor += 1;
            if let Some(value) = self.expression(0) {
                self.inspect_expression(&value);
                return Some(Statement::Assign {
                    name,
                    value,
                    declaration,
                });
            }
            return None;
        }
        let expression = self.expression(0)?;
        let assignment_operator = ["=", ":=", "+=", "-=", "*=", "/="]
            .into_iter()
            .find(|operator| self.take_operator(operator));
        if let Some(assignment_operator) = assignment_operator {
            if let Expr::Name(name) = expression {
                let mut value = self.expression(0)?;
                if assignment_operator.len() == 2
                    && assignment_operator.ends_with('=')
                    && assignment_operator != ":="
                {
                    value = Expr::Binary {
                        left: Box::new(Expr::Name(name.clone())),
                        operator: assignment_operator[..1].to_string(),
                        right: Box::new(value),
                    };
                }
                self.inspect_expression(&value);
                return Some(Statement::Assign {
                    name,
                    value,
                    declaration,
                });
            }
            let token = self.tokens[start].clone();
            self.diagnostics.push(Diagnostic::error(
                token.line,
                token.column,
                "Only named variables can be assigned.",
            ));
            return None;
        }
        self.inspect_expression(&expression);
        Some(Statement::Expression(expression))
    }

    fn block(&mut self) -> Option<Vec<Statement>> {
        if !self.take(&Kind::Newline) || !self.take(&Kind::Indent) {
            self.error_here("Expected an indented block after this statement.");
            return None;
        }
        let mut body = Vec::new();
        while !self.is(&Kind::Dedent) && !self.is(&Kind::End) {
            if self.take(&Kind::Newline) {
                continue;
            }
            if let Some(statement) = self.statement() {
                body.push(statement);
            } else {
                self.recover_line();
            }
            if body.len() > crate::runtime::MAX_STATEMENTS {
                self.error_here("Pine block has too many statements.");
                break;
            }
        }
        if !self.take(&Kind::Dedent) {
            self.error_here("Expected the end of the indented block.");
            return None;
        }
        Some(body)
    }

    fn take_identifier(&mut self, name: &str) -> bool {
        if self.peek_identifier(name) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn inspect_expression(&mut self, expression: &Expr) {
        if let Expr::Call { name, arguments } = expression {
            if matches!(name.as_str(), "indicator" | "strategy" | "library") {
                self.declaration_count += 1;
            }
            if is_indicator_output_call(name) {
                self.visual_output_seen = true;
            }
            if name == "strategy" {
                self.strategy_seen = true;
            }
            if name == "indicator" {
                self.indicator_seen = true;
                if let Some(Expr::String(value)) = arguments.first().map(|argument| &argument.value)
                {
                    self.name = value.clone();
                }
                self.overlay = arguments.iter().find(|argument| argument.name.as_deref() == Some("overlay"))
                    .is_some_and(|argument| matches!(argument.value, Expr::Name(ref value) if value == "true"));
            }
            if name == "strategy" {
                if let Some(Expr::String(value)) = arguments.first().map(|argument| &argument.value)
                {
                    self.name = value.clone();
                }
                self.overlay = arguments.iter().find(|argument| argument.name.as_deref() == Some("overlay"))
                    .is_some_and(|argument| matches!(argument.value, Expr::Name(ref value) if value == "true"));
            }
        }
        if let Expr::Call { name, .. } = expression {
            if name != "request.security" && name.starts_with("request.") {
                self.diagnostics.push(Diagnostic::error(self.current().line, self.current().column, "`request.*` is unsupported because only approved selected-app history is available to this runtime."));
            } else if !supported_function(name) {
                self.diagnostics.push(Diagnostic::error(
                    self.current().line,
                    self.current().column,
                    format!("Unsupported function `{name}()`; no approximation was applied."),
                ));
            } else if matches!(name.as_str(), "ta.macd" | "ta.dmi" | "ta.bb" | "ta.stoch") {
                self.diagnostics.push(Diagnostic::error(
                    self.current().line,
                    self.current().column,
                    format!(
                        "`{name}()` returns multiple series, which this runtime cannot preserve."
                    ),
                ));
            }
            if let Expr::Call { arguments, .. } = expression {
                for argument in arguments {
                    self.inspect_expression(&argument.value);
                }
            }
        } else {
            match expression {
                Expr::Unary { value, .. } | Expr::Index { value, .. } => {
                    self.inspect_expression(value)
                }
                Expr::Binary { left, right, .. } => {
                    self.inspect_expression(left);
                    self.inspect_expression(right);
                }
                Expr::Conditional { condition, yes, no } => {
                    self.inspect_expression(condition);
                    self.inspect_expression(yes);
                    self.inspect_expression(no);
                }
                Expr::Number(_) | Expr::String(_) | Expr::Name(_) => {}
                Expr::Call { .. } => unreachable!(),
            }
        }
    }

    fn validate_names(&mut self, expression: &Expr, declared: &HashSet<String>) {
        match expression {
            Expr::Name(name) if !known_name(name) && !declared.contains(name.as_str()) => {
                let token = self.current();
                self.diagnostics.push(Diagnostic::error(token.line, token.column, format!("Unsupported Pine identifier `{name}`; it would otherwise evaluate as missing.")));
            }
            Expr::Unary { value, .. } | Expr::Index { value, .. } => {
                self.validate_names(value, declared)
            }
            Expr::Binary { left, right, .. } => {
                self.validate_names(left, declared);
                self.validate_names(right, declared);
            }
            Expr::Conditional { condition, yes, no } => {
                self.validate_names(condition, declared);
                self.validate_names(yes, declared);
                self.validate_names(no, declared);
            }
            Expr::Call { arguments, .. } => {
                for argument in arguments {
                    self.validate_names(&argument.value, declared);
                }
            }
            Expr::Number(_) | Expr::String(_) | Expr::Name(_) => {}
        }
    }

    fn expression(&mut self, minimum: u8) -> Option<Expr> {
        self.expression_depth += 1;
        if self.expression_depth > 128 {
            self.expression_depth -= 1;
            self.error_here("Expression nesting exceeds the 128-level parser limit.");
            return None;
        }
        let result = self.expression_inner(minimum);
        self.expression_depth -= 1;
        result
    }

    fn expression_inner(&mut self, minimum: u8) -> Option<Expr> {
        let mut left = self.prefix()?;
        loop {
            if self.is(&Kind::LeftBracket) {
                if 8 < minimum {
                    break;
                }
                self.cursor += 1;
                let offset = self.expression(0)?;
                if !self.take(&Kind::RightBracket) {
                    self.error_here("Expected `]` after history offset.");
                    return None;
                }
                left = Expr::Index {
                    value: Box::new(left),
                    offset: Box::new(offset),
                };
                continue;
            }
            if self.is(&Kind::Dot) {
                if 8 < minimum {
                    break;
                }
                self.cursor += 1;
                let Some(member) = self.current_identifier().map(str::to_string) else {
                    self.error_here("Expected a member name after `.`.");
                    return None;
                };
                self.cursor += 1;
                match left {
                    Expr::Name(base) => left = Expr::Name(format!("{base}.{member}")),
                    _ => {
                        self.error_here("Member access on runtime values is unsupported.");
                        return None;
                    }
                }
                continue;
            }
            if self.is(&Kind::LeftParen) {
                if 8 < minimum {
                    break;
                }
                let Expr::Name(name) = left else {
                    self.error_here("Only named functions can be called.");
                    return None;
                };
                self.cursor += 1;
                let mut arguments = Vec::new();
                while !self.is(&Kind::RightParen) && !self.is(&Kind::End) {
                    let named = self
                        .current_identifier()
                        .map(str::to_string)
                        .filter(|_| self.operator_after_current("="));
                    if named.is_some() {
                        self.cursor += 1;
                        self.cursor += 1;
                    }
                    let value = self.expression(0)?;
                    arguments.push(Argument { name: named, value });
                    if !self.take(&Kind::Comma) {
                        break;
                    }
                }
                if !self.take(&Kind::RightParen) {
                    self.error_here("Expected `)` to close function call.");
                    return None;
                }
                left = Expr::Call { name, arguments };
                continue;
            }
            if self.is(&Kind::Question) {
                if 1 < minimum {
                    break;
                }
                self.cursor += 1;
                let yes = self.expression(0)?;
                if !self.take(&Kind::Colon) {
                    self.error_here("Expected `:` in conditional expression.");
                    return None;
                }
                let no = self.expression(1)?;
                left = Expr::Conditional {
                    condition: Box::new(left),
                    yes: Box::new(yes),
                    no: Box::new(no),
                };
                continue;
            }
            let Some((operator, precedence)) = self.current_operator() else {
                break;
            };
            if precedence < minimum {
                break;
            }
            self.cursor += 1;
            let right = self.expression(precedence + 1)?;
            left = Expr::Binary {
                left: Box::new(left),
                operator,
                right: Box::new(right),
            };
        }
        Some(left)
    }

    fn prefix(&mut self) -> Option<Expr> {
        let token = self.current().clone();
        match token.kind {
            Kind::Number(value) => {
                self.cursor += 1;
                Some(Expr::Number(value))
            }
            Kind::String(value) => {
                self.cursor += 1;
                Some(Expr::String(value))
            }
            Kind::Identifier(name) => {
                self.cursor += 1;
                if name == "not" {
                    Some(Expr::Unary {
                        operator: name,
                        value: Box::new(self.expression(7)?),
                    })
                } else {
                    Some(Expr::Name(name))
                }
            }
            Kind::Operator(operator) if operator == "-" || operator == "+" => {
                self.cursor += 1;
                Some(Expr::Unary {
                    operator,
                    value: Box::new(self.expression(7)?),
                })
            }
            Kind::LeftParen => {
                self.cursor += 1;
                let value = self.expression(0)?;
                if !self.take(&Kind::RightParen) {
                    self.error_here("Expected `)`.");
                    return None;
                }
                Some(value)
            }
            _ => {
                self.error_here("Expected an expression.");
                None
            }
        }
    }

    fn current_operator(&self) -> Option<(String, u8)> {
        let operator = match &self.current().kind {
            Kind::Operator(value) => value.as_str(),
            Kind::Identifier(value) if value == "and" || value == "or" => value.as_str(),
            _ => return None,
        };
        let precedence = match operator {
            "or" | "||" => 1,
            "and" | "&&" => 2,
            "==" | "!=" | "<" | ">" | "<=" | ">=" => 3,
            "+" | "-" => 4,
            "*" | "/" | "%" => 5,
            _ => return None,
        };
        Some((operator.to_string(), precedence))
    }
    fn current(&self) -> &Token {
        &self.tokens[self.cursor.min(self.tokens.len() - 1)]
    }
    fn is(&self, kind: &Kind) -> bool {
        &self.current().kind == kind
    }
    fn take(&mut self, kind: &Kind) -> bool {
        if self.is(kind) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }
    fn current_identifier(&self) -> Option<&str> {
        if let Kind::Identifier(value) = &self.current().kind {
            Some(value)
        } else {
            None
        }
    }
    fn next_identifier(&self) -> Option<&str> {
        match self.tokens.get(self.cursor + 1).map(|token| &token.kind) {
            Some(Kind::Identifier(value)) => Some(value),
            _ => None,
        }
    }
    fn peek_identifier(&self, name: &str) -> bool {
        self.current_identifier() == Some(name)
    }
    fn operator_after_current(&self, operator: &str) -> bool {
        matches!(self.tokens.get(self.cursor + 1).map(|token| &token.kind), Some(Kind::Operator(value)) if value == operator)
    }
    fn operator_after_next(&self, operator: &str) -> bool {
        matches!(self.tokens.get(self.cursor + 2).map(|token| &token.kind), Some(Kind::Operator(value)) if value == operator)
    }
    fn take_operator(&mut self, operator: &str) -> bool {
        if matches!(&self.current().kind, Kind::Operator(value) if value == operator) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }
    fn error_here(&mut self, message: impl Into<String>) {
        let token = self.current();
        self.diagnostics
            .push(Diagnostic::error(token.line, token.column, message));
    }
    fn recover_line(&mut self) {
        while !self.is(&Kind::Newline) && !self.is(&Kind::End) {
            self.cursor += 1;
        }
        self.take(&Kind::Newline);
    }
}

fn is_indicator_output_call(name: &str) -> bool {
    name.starts_with("plot")
        || matches!(name, "barcolor" | "bgcolor" | "hline" | "alertcondition")
        || ["line.", "label.", "box.", "table.", "polyline."]
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

fn collect_declared_names(statements: &[Statement], names: &mut HashSet<String>) {
    for statement in statements {
        match statement {
            Statement::Assign { name, .. } => {
                names.insert(name.clone());
            }
            Statement::If {
                body, otherwise, ..
            } => {
                collect_declared_names(body, names);
                collect_declared_names(otherwise, names);
            }
            Statement::For { name, body, .. } => {
                names.insert(name.clone());
                collect_declared_names(body, names);
            }
            Statement::Expression(_) => {}
        }
    }
}

fn validate_statements(parser: &mut Parser, statements: &[Statement], declared: &HashSet<String>) {
    for statement in statements {
        match statement {
            Statement::Assign { value, .. } | Statement::Expression(value) => {
                parser.validate_names(value, declared)
            }
            Statement::If {
                condition,
                body,
                otherwise,
            } => {
                parser.validate_names(condition, declared);
                validate_statements(parser, body, declared);
                validate_statements(parser, otherwise, declared);
            }
            Statement::For {
                start, end, body, ..
            } => {
                parser.validate_names(start, declared);
                parser.validate_names(end, declared);
                validate_statements(parser, body, declared);
            }
        }
    }
}

fn supported_function(name: &str) -> bool {
    matches!(
        name,
        "indicator"
            | "strategy"
            | "strategy.entry"
            | "strategy.order"
            | "strategy.exit"
            | "strategy.close"
            | "strategy.close_all"
            | "strategy.cancel"
            | "strategy.cancel_all"
            | "strategy.opentrades.entry_id"
            | "strategy.opentrades.entry_price"
            | "strategy.opentrades.entry_bar_index"
            | "strategy.opentrades.entry_time"
            | "strategy.opentrades.commission"
            | "strategy.opentrades.profit"
            | "strategy.opentrades.profit_percent"
            | "strategy.opentrades.max_runup"
            | "strategy.opentrades.max_drawdown"
            | "strategy.opentrades.max_runup_percent"
            | "strategy.opentrades.max_drawdown_percent"
            | "strategy.opentrades.size"
            | "strategy.closedtrades.entry_bar_index"
            | "strategy.closedtrades.entry_time"
            | "strategy.closedtrades.exit_bar_index"
            | "strategy.closedtrades.exit_time"
            | "strategy.closedtrades.entry_price"
            | "strategy.closedtrades.exit_price"
            | "strategy.closedtrades.profit"
            | "strategy.closedtrades.profit_percent"
            | "strategy.closedtrades.max_runup"
            | "strategy.closedtrades.max_drawdown"
            | "strategy.closedtrades.max_runup_percent"
            | "strategy.closedtrades.max_drawdown_percent"
            | "strategy.closedtrades.size"
            | "strategy.closedtrades.commission"
            | "strategy.closedtrades.entry_id"
            | "plot"
            | "plotshape"
            | "plotchar"
            | "hline"
            | "fill"
            | "alertcondition"
            | "barcolor"
            | "bgcolor"
            | "na"
            | "nz"
            | "fixnan"
            | "color.new"
            | "str.tostring"
            | "str.tonumber"
            | "math.abs"
            | "math.floor"
            | "math.ceil"
            | "math.round"
            | "math.sqrt"
            | "math.log"
            | "math.exp"
            | "math.max"
            | "math.min"
            | "math.pow"
            | "ta.sma"
            | "ta.ema"
            | "ta.rma"
            | "ta.wma"
            | "ta.vwma"
            | "ta.rsi"
            | "ta.atr"
            | "ta.tr"
            | "ta.highest"
            | "ta.lowest"
            | "ta.highestbars"
            | "ta.lowestbars"
            | "ta.stdev"
            | "ta.dev"
            | "ta.mom"
            | "ta.change"
            | "ta.crossover"
            | "ta.crossunder"
            | "ta.cross"
            | "ta.barssince"
            | "ta.valuewhen"
            | "ta.macd"
            | "ta.dmi"
            | "ta.bb"
            | "ta.stoch"
            | "request.security"
    ) || name.starts_with("input.")
}

fn known_name(name: &str) -> bool {
    matches!(
        name,
        "true"
            | "false"
            | "na"
            | "open"
            | "high"
            | "low"
            | "close"
            | "volume"
            | "hl2"
            | "hlc3"
            | "ohlc4"
            | "bar_index"
            | "time"
            | "barstate.isfirst"
            | "barstate.islast"
            | "barstate.isconfirmed"
            | "barstate.isrealtime"
            | "syminfo.tickerid"
            | "syminfo.ticker"
            | "timeframe.period"
            | "strategy.long"
            | "strategy.short"
            | "strategy.direction.long"
            | "strategy.direction.short"
            | "strategy.percent_of_equity"
            | "strategy.fixed"
            | "strategy.cash"
            | "strategy.oca.cancel"
            | "strategy.oca.reduce"
            | "strategy.oca.none"
            | "currency.NONE"
            | "strategy.commission.percent"
            | "strategy.commission.cash_per_order"
            | "strategy.commission.cash_per_contract"
            | "strategy.position_size"
            | "strategy.position_avg_price"
            | "strategy.position_entry_name"
            | "strategy.initial_capital"
            | "strategy.closedtrades"
            | "strategy.wintrades"
            | "strategy.losstrades"
            | "strategy.grossprofit"
            | "strategy.grossprofit_percent"
            | "strategy.grossloss"
            | "strategy.grossloss_percent"
            | "strategy.netprofit"
            | "strategy.netprofit_percent"
            | "strategy.openprofit"
            | "strategy.openprofit_percent"
            | "strategy.max_runup"
            | "strategy.max_runup_percent"
            | "strategy.max_drawdown"
            | "strategy.max_drawdown_percent"
            | "strategy.opentrades"
            | "color.black"
            | "color.white"
            | "color.red"
            | "color.green"
            | "color.lime"
            | "color.blue"
            | "color.orange"
            | "color.yellow"
            | "color.aqua"
            | "color.fuchsia"
            | "color.purple"
            | "color.gray"
            | "color.maroon"
            | "color.navy"
            | "color.olive"
            | "color.silver"
            | "color.teal"
            | "math.pi"
            | "math.e"
            | "plot.style_line"
            | "display.all"
            | "format.price"
    ) || name.starts_with("input.")
        || name.starts_with("plot.style_")
        || name.starts_with("display.")
        || name.starts_with("format.")
        || name.starts_with("location.")
        || name.starts_with("shape.")
        || name.starts_with("size.")
        || name.starts_with("text.align_")
        || name.starts_with("barmerge.")
}
