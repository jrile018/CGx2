use std::collections::{BTreeMap, BTreeSet, VecDeque};

use cgx2_syntax::Span;

use super::model::*;

type Check<T = ()> = Result<T, Diagnostic>;

fn error(code: &'static str, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        code,
        message: message.into(),
        span,
        related: Vec::new(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Origin {
    parameter: Option<usize>,
    projection: Vec<Projection>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Value {
    must_live: bool,
    may_live: bool,
    moved: BTreeSet<Vec<Projection>>,
    origins: BTreeSet<Origin>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum LoanId {
    Local(String),
    Foreign(String, usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Loan {
    places: BTreeSet<Place>,
    kind: LoanKind,
    definite: bool,
    span: Span,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct State {
    values: BTreeMap<String, Value>,
    loans: BTreeMap<LoanId, Loan>,
    must_open: BTreeSet<String>,
    may_open: BTreeSet<String>,
}

impl State {
    fn entries(&self) -> usize {
        let mut count = self.must_open.len().saturating_add(self.may_open.len());
        for value in self.values.values() {
            count = count.saturating_add(1);
            for path in &value.moved {
                count = count.saturating_add(1 + path.len());
            }
            for origin in &value.origins {
                count = count.saturating_add(1 + origin.projection.len());
            }
        }
        for loan in self.loans.values() {
            count = count.saturating_add(1);
            for place in &loan.places {
                count = count.saturating_add(1 + place.projection.len());
            }
        }
        count
    }

    fn join(&mut self, other: &Self, span: Span) -> Check<bool> {
        let before = self.clone();
        for (name, incoming) in &other.values {
            let value = self.values.get_mut(name).expect("validated local set");
            value.must_live &= incoming.must_live;
            value.may_live |= incoming.may_live;
            value.moved.extend(incoming.moved.iter().cloned());
            value.origins.extend(incoming.origins.iter().cloned());
        }
        let ids: BTreeSet<_> = self
            .loans
            .keys()
            .chain(other.loans.keys())
            .cloned()
            .collect();
        for id in ids {
            match (self.loans.get_mut(&id), other.loans.get(&id)) {
                (Some(a), Some(b)) => {
                    if a.kind != b.kind {
                        return Err(error("CGO105", "loan kind differs across CFG edges", span));
                    }
                    a.definite &= b.definite;
                    a.places.extend(b.places.iter().cloned());
                }
                (Some(a), None) => a.definite = false,
                (None, Some(b)) => {
                    let mut b = b.clone();
                    b.definite = false;
                    self.loans.insert(id, b);
                }
                (None, None) => unreachable!(),
            }
        }
        self.must_open.retain(|r| other.must_open.contains(r));
        self.may_open.extend(other.may_open.iter().cloned());
        Ok(*self != before)
    }
}

struct Analyzer<'a> {
    function: &'a Function,
    summaries: BTreeMap<&'a str, &'a FunctionSummary>,
    external_regions: BTreeSet<String>,
    limits: AnalysisLimits,
    stats: AnalysisStats,
}

/// Analyze one local body. `summaries` excludes this function's own summary,
/// which is registered automatically (including recursive calls). No inference
/// or recursive interprocedural expansion is performed.
pub fn analyze_function(
    function: &Function,
    summaries: &[FunctionSummary],
    source: &str,
    limits: AnalysisLimits,
) -> AnalysisReport {
    let mut analyzer = Analyzer {
        function,
        summaries: BTreeMap::new(),
        external_regions: BTreeSet::new(),
        limits,
        stats: AnalysisStats::default(),
    };
    let result = analyzer
        .validate(summaries, source)
        .and_then(|()| analyzer.run());
    match result {
        Ok(()) => AnalysisReport {
            complete: true,
            diagnostics: Vec::new(),
            stats: analyzer.stats,
        },
        Err(diagnostic) => AnalysisReport {
            complete: diagnostic.code != "CGO106",
            diagnostics: vec![diagnostic],
            stats: analyzer.stats,
        },
    }
}

impl<'a> Analyzer<'a> {
    fn limit(&self, message: &str, span: Span) -> Diagnostic {
        error("CGO106", message, span)
    }

    fn name(&self, name: &str, span: Span) -> Check {
        if name.is_empty() {
            return Err(error("CGO105", "empty identifier", span));
        }
        if name.len() > self.limits.max_name_bytes {
            return Err(self.limit("identifier byte limit exceeded", span));
        }
        Ok(())
    }

    fn path(&self, path: &[Projection], span: Span) -> Check {
        if path.len() > self.limits.max_projection_depth {
            return Err(self.limit("projection depth limit exceeded", span));
        }
        for projection in path {
            if let Projection::Field(name) = projection {
                self.name(name, span)?;
            }
        }
        Ok(())
    }

    fn place(&self, place: &Place, span: Span) -> Check {
        self.local(&place.local, span)?;
        self.path(&place.projection, span)
    }

    fn local(&self, name: &str, span: Span) -> Check<&Local> {
        self.function
            .locals
            .iter()
            .find(|l| l.name == name)
            .ok_or_else(|| error("CGO105", format!("unknown local slot '{name}'"), span))
    }

    fn summary(&self, summary: &FunctionSummary, span: Span) -> Check {
        self.name(&summary.name, span)?;
        if let ResultMode::OwnedResult {
            from_parameter: Some(i),
        } = &summary.result
        {
            if summary.parameters.get(*i) != Some(&ParameterMode::OwnedResult) {
                return Err(error(
                    "CGO105",
                    "conditional result parameter provenance mismatch",
                    span,
                ));
            }
        }
        if summary.parameters.len() > self.limits.max_locals {
            return Err(self.limit("summary parameter limit exceeded", span));
        }
        match &summary.result {
            ResultMode::Owned {
                from_parameter: Some(i),
            } => {
                if summary.parameters.get(*i) != Some(&ParameterMode::Owned) {
                    return Err(error(
                        "CGO105",
                        "owned result must derive from an owned parameter",
                        span,
                    ));
                }
            }
            ResultMode::Loan {
                parameter,
                projection,
                kind,
            } => {
                self.path(projection, span)?;
                match (summary.parameters.get(*parameter), kind) {
                    (Some(ParameterMode::Shared), LoanKind::Shared)
                    | (Some(ParameterMode::Exclusive), _) => {}
                    _ => {
                        return Err(error(
                            "CGO105",
                            "result loan must derive from a compatible borrowed parameter",
                            span,
                        ))
                    }
                }
            }
            _ => {}
        }
        if let Retention::UntilRelease { parameters } = &summary.retention {
            if parameters.is_empty() || parameters.len() > summary.parameters.len() {
                return Err(error("CGO105", "invalid retained parameter list", span));
            }
            let mut seen = BTreeSet::new();
            for i in parameters {
                if !seen.insert(i)
                    || !matches!(
                        summary.parameters.get(*i),
                        Some(ParameterMode::Shared | ParameterMode::Exclusive)
                    )
                {
                    return Err(error(
                        "CGO105",
                        "retention must name distinct borrowed parameters",
                        span,
                    ));
                }
                if let ResultMode::Loan {
                    parameter, kind, ..
                } = summary.result
                {
                    if parameter == *i
                        && (kind == LoanKind::Exclusive
                            || summary.parameters[*i] == ParameterMode::Exclusive)
                    {
                        return Err(error(
                            "CGO102",
                            "exclusive retained argument overlaps result loan",
                            span,
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn validate(&mut self, summaries: &'a [FunctionSummary], source: &str) -> Check {
        let zero = Span { start: 0, end: 0 };
        let limits = self.limits;
        if [
            limits.max_blocks,
            limits.max_operations,
            limits.max_locals,
            limits.max_summaries,
            limits.max_projection_depth,
            limits.max_name_bytes,
            limits.max_steps,
            limits.max_state_entries,
            limits.max_source_bytes,
        ]
        .contains(&0)
        {
            return Err(self.limit("analysis limits must be positive", zero));
        }
        if source.len() > limits.max_source_bytes {
            return Err(self.limit("source byte limit exceeded", zero));
        }
        let valid_span = |span: Span| -> Check {
            if span.start > span.end
                || span.end > source.len()
                || !source.is_char_boundary(span.start)
                || !source.is_char_boundary(span.end)
            {
                Err(error("CGO105", "invalid UTF-8 byte span", zero))
            } else {
                Ok(())
            }
        };
        let f = self.function;
        valid_span(f.span)?;
        if f.blocks.len() > limits.max_blocks
            || f.locals.len() > limits.max_locals
            || summaries.len() >= limits.max_summaries
        {
            return Err(self.limit("input shape limit exceeded", f.span));
        }
        if f.entry >= f.blocks.len()
            || f.summary.parameters.len() > f.locals.len()
            || f.summary.retention != Retention::Local
        {
            return Err(error(
                "CGO105",
                "invalid local function entry/parameters/retention",
                f.span,
            ));
        }
        for summary in std::iter::once(&f.summary).chain(summaries) {
            self.summary(summary, f.span)?;
            if self.summaries.insert(&summary.name, summary).is_some() {
                return Err(error("CGO105", "duplicate function summary", f.span));
            }
        }
        let mut names = BTreeSet::new();
        for (i, local) in f.locals.iter().enumerate() {
            valid_span(local.span)?;
            self.name(&local.name, local.span)?;
            if !names.insert(&local.name) {
                return Err(error("CGO105", "duplicate local slot", local.span));
            }
            if i < f.summary.parameters.len()
                && (local.storage == Storage::ConditionalOwned)
                    != (f.summary.parameters[i] == ParameterMode::OwnedResult)
            {
                return Err(error(
                    "CGO105",
                    "conditional parameter storage mismatch",
                    local.span,
                ));
            }
            if let Storage::Region(region) = &local.storage {
                self.name(region, local.span)?;
                if i < f.summary.parameters.len() {
                    self.external_regions.insert(region.clone());
                }
            }
        }
        let mut operations = 0usize;
        for block in &f.blocks {
            valid_span(block.span)?;
            operations = operations.saturating_add(block.operations.len());
            if operations > limits.max_operations {
                return Err(self.limit("operation count limit exceeded", block.span));
            }
            for op in &block.operations {
                valid_span(op.span)?;
                self.validate_op(&op.operation, op.span)?;
            }
            match &block.terminator {
                Terminator::MatchResult {
                    from,
                    payload,
                    ok,
                    err,
                } => {
                    self.place(from, block.span)?;
                    self.local(payload, block.span)?;
                    if *ok >= f.blocks.len() || *err >= f.blocks.len() {
                        return Err(error("CGO105", "invalid conditional successor", block.span));
                    }
                }
                Terminator::Return(ReturnValue::OwnedResult(place)) => {
                    self.place(place, block.span)?
                }
                Terminator::Goto(i) if *i >= f.blocks.len() => {
                    return Err(error("CGO105", "invalid CFG successor", block.span))
                }
                Terminator::Branch {
                    then_block,
                    else_block,
                } if *then_block >= f.blocks.len() || *else_block >= f.blocks.len() => {
                    return Err(error("CGO105", "invalid CFG successor", block.span))
                }
                Terminator::Return(ReturnValue::Owned(place)) => self.place(place, block.span)?,
                Terminator::Return(ReturnValue::Loan(token)) => self.name(token, block.span)?,
                _ => {}
            }
        }
        Ok(())
    }

    fn validate_op(&self, op: &Operation, span: Span) -> Check {
        match op {
            Operation::TryCreate(name) => self.local(name, span).map(|_| ()),
            Operation::MoveResult { from, to } => {
                self.place(from, span)?;
                self.local(to, span).map(|_| ())
            }
            Operation::DropResult(place) => self.place(place, span),
            Operation::OpenRegion(name)
            | Operation::CloseRegion(name)
            | Operation::EndLoan(name)
            | Operation::ReleaseForeign(name) => self.name(name, span),
            Operation::Create(name) | Operation::DropRemainder(name) => {
                self.local(name, span).map(|_| ())
            }
            Operation::Read(place) | Operation::Write(place) | Operation::Drop(place) => {
                self.place(place, span)
            }
            Operation::Move { from, to } => {
                self.place(from, span)?;
                self.local(to, span).map(|_| ())
            }
            Operation::Borrow { place, token, .. } => {
                self.place(place, span)?;
                self.name(token, span)
            }
            Operation::UseLoan { token, .. } => self.name(token, span),
            Operation::Call {
                function,
                arguments,
                result,
                retention_owner,
            } => {
                self.name(function, span)?;
                if arguments.len() > self.limits.max_locals {
                    return Err(self.limit("call argument limit exceeded", span));
                }
                for arg in arguments {
                    self.place(arg, span)?;
                }
                match result {
                    CallResult::Unit => {}
                    CallResult::Owned(name) | CallResult::OwnedResult(name) => {
                        self.local(name, span)?;
                    }
                    CallResult::Loan(token) => self.name(token, span)?,
                }
                if let Some(owner) = retention_owner {
                    self.name(owner, span)?;
                }
                Ok(())
            }
            Operation::Unsupported(feature) => Err(error(
                "CGO105",
                format!("unsupported ownership operation: {feature}"),
                span,
            )),
        }
    }

    fn step(&mut self, span: Span) -> Check {
        if self.stats.steps >= self.limits.max_steps {
            return Err(self.limit("analysis step budget exhausted", span));
        }
        self.stats.steps += 1;
        Ok(())
    }

    fn space(&self, states: &[Option<State>], working: &State, span: Span) -> Check {
        let entries = states
            .iter()
            .flatten()
            .fold(working.entries(), |n, s| n.saturating_add(s.entries()));
        if entries > self.limits.max_state_entries {
            Err(self.limit("analysis state budget exhausted", span))
        } else {
            Ok(())
        }
    }

    fn run(&mut self) -> Check {
        let mut initial = State {
            must_open: self.external_regions.clone(),
            may_open: self.external_regions.clone(),
            ..State::default()
        };
        for (i, local) in self.function.locals.iter().enumerate() {
            let parameter = i < self.function.summary.parameters.len();
            initial.values.insert(
                local.name.clone(),
                Value {
                    must_live: parameter,
                    may_live: parameter,
                    origins: if parameter {
                        BTreeSet::from([Origin {
                            parameter: Some(i),
                            projection: Vec::new(),
                        }])
                    } else {
                        BTreeSet::new()
                    },
                    ..Value::default()
                },
            );
        }
        let mut entries = vec![None; self.function.blocks.len()];
        entries[self.function.entry] = Some(initial);
        let mut pending = VecDeque::from([self.function.entry]);
        let mut queued = BTreeSet::from([self.function.entry]);
        while let Some(index) = pending.pop_front() {
            queued.remove(&index);
            let block = &self.function.blocks[index];
            let mut state = entries[index].clone().expect("queued reachable block");
            self.space(&entries, &state, block.span)?;
            self.stats.block_visits += 1;
            for op in &block.operations {
                self.step(op.span)?;
                self.operation(&mut state, &op.operation, op.span)?;
                self.space(&entries, &state, op.span)?;
            }
            self.step(block.span)?;
            let successors = match &block.terminator {
                Terminator::Return(value) => {
                    self.return_value(&mut state, value, block.span)?;
                    Vec::new()
                }
                Terminator::Goto(next) => vec![(*next, state.clone())],
                Terminator::Branch {
                    then_block,
                    else_block,
                } => vec![(*then_block, state.clone()), (*else_block, state.clone())],
                Terminator::MatchResult {
                    from,
                    payload,
                    ok,
                    err,
                } => {
                    self.sum_live(&state, from, block.span)?;
                    let origins = self.origins(&state, from, block.span)?;
                    self.consume(&mut state, from, block.span)?;
                    let failure = state.clone();
                    self.destination(&state, payload, &Storage::Owned, block.span)?;
                    self.initialize(&mut state, payload, origins);
                    vec![(*ok, state.clone()), (*err, failure)]
                }
            };
            for (next, state) in successors {
                self.step(block.span)?;
                let changed = match &mut entries[next] {
                    Some(entry) => entry.join(&state, self.function.blocks[next].span)?,
                    slot @ None => {
                        *slot = Some(state.clone());
                        true
                    }
                };
                self.space(&entries, &state, block.span)?;
                if changed && queued.insert(next) {
                    pending.push_back(next);
                }
            }
        }
        Ok(())
    }

    fn mode(&self, name: &str) -> ParameterMode {
        self.function
            .locals
            .iter()
            .position(|l| l.name == name)
            .and_then(|i| self.function.summary.parameters.get(i).copied())
            .unwrap_or_else(|| {
                if self
                    .function
                    .locals
                    .iter()
                    .any(|l| l.name == name && l.storage == Storage::ConditionalOwned)
                {
                    ParameterMode::OwnedResult
                } else {
                    ParameterMode::Owned
                }
            })
    }

    fn live<'s>(&self, state: &'s State, place: &Place, span: Span) -> Check<&'s Value> {
        let value = &state.values[&place.local];
        if !value.must_live
            || value
                .moved
                .iter()
                .any(|path| paths_overlap(path, &place.projection))
        {
            return Err(error(
                "CGO101",
                format!("place '{place:?}' is uninitialized, moved, or only possibly live"),
                span,
            ));
        }
        Ok(value)
    }

    fn conflict(&self, state: &State, place: &Place, exclusive: bool, span: Span) -> Check {
        for loan in state.loans.values() {
            if (exclusive || loan.kind == LoanKind::Exclusive)
                && loan.places.iter().any(|p| place.overlaps(p))
            {
                let mut diagnostic =
                    error("CGO102", format!("conflicting access to '{place:?}'"), span);
                diagnostic.related.push(loan.span);
                return Err(diagnostic);
            }
        }
        Ok(())
    }

    fn access(&self, state: &State, place: &Place, mode: ParameterMode, span: Span) -> Check {
        if mode == ParameterMode::OwnedResult {
            return self.sum_live(state, place, span);
        }
        if self.local(&place.local, span)?.storage == Storage::ConditionalOwned {
            return Err(error(
                "CGO105",
                "conditional owner requires discriminant refinement",
                span,
            ));
        }
        self.live(state, place, span)?;
        let capability = self.mode(&place.local);
        if (mode == ParameterMode::Owned && capability != ParameterMode::Owned)
            || (mode == ParameterMode::Exclusive && capability == ParameterMode::Shared)
        {
            return Err(error(
                "CGO102",
                "parameter access cannot transfer ownership or strengthen a shared loan",
                span,
            ));
        }
        self.conflict(state, place, mode != ParameterMode::Shared, span)
    }

    fn origins(&self, state: &State, place: &Place, span: Span) -> Check<BTreeSet<Origin>> {
        let mut origins = BTreeSet::new();
        for origin in &state.values[&place.local].origins {
            let mut origin = origin.clone();
            origin.projection.extend(place.projection.iter().cloned());
            self.path(&origin.projection, span)?;
            origins.insert(origin);
        }
        Ok(origins)
    }

    fn consume(&self, state: &mut State, place: &Place, span: Span) -> Check {
        if place.projection.contains(&Projection::DynamicIndex) {
            return Err(error(
                "CGO105",
                "owned move/drop through dynamic index is unsupported",
                span,
            ));
        }
        let value = state.values.get_mut(&place.local).expect("validated local");
        value.moved.insert(place.projection.clone());
        if place.projection.is_empty() {
            value.must_live = false;
            value.may_live = false;
        }
        Ok(())
    }

    fn destination(&self, state: &State, name: &str, storage: &Storage, span: Span) -> Check {
        if self.mode(name) != ParameterMode::Owned || state.values[name].may_live {
            return Err(error(
                "CGO101",
                "destination slot may already be live or is a borrowed parameter",
                span,
            ));
        }
        if &self.local(name, span)?.storage != storage {
            return Err(error(
                "CGO103",
                "implicit cross-region transfer is forbidden",
                span,
            ));
        }
        if let Storage::Region(region) = storage {
            if !state.must_open.contains(region) {
                return Err(error(
                    "CGO103",
                    "destination region is not definitely open",
                    span,
                ));
            }
        }
        self.conflict(state, &Place::local(name), true, span)
    }

    fn sum_live(&self, state: &State, place: &Place, span: Span) -> Check {
        if self.local(&place.local, span)?.storage != Storage::ConditionalOwned
            || !place.projection.is_empty()
        {
            return Err(error(
                "CGO105",
                "expected whole conditional-owner slot",
                span,
            ));
        }
        self.live(state, place, span)?;
        self.conflict(state, place, true, span)
    }
    fn sum_destination(&self, state: &State, name: &str, span: Span) -> Check {
        if self.local(name, span)?.storage != Storage::ConditionalOwned
            || state.values[name].may_live
        {
            return Err(error(
                "CGO101",
                "conditional destination is occupied or wrong storage",
                span,
            ));
        }
        self.conflict(state, &Place::local(name), true, span)
    }

    fn initialize(&self, state: &mut State, name: &str, origins: BTreeSet<Origin>) {
        state.values.insert(
            name.to_owned(),
            Value {
                must_live: true,
                may_live: true,
                origins,
                moved: BTreeSet::new(),
            },
        );
    }

    fn add_loan(
        &self,
        state: &mut State,
        id: LoanId,
        place: Place,
        kind: LoanKind,
        span: Span,
    ) -> Check {
        if state.loans.contains_key(&id) {
            return Err(error(
                "CGO102",
                "loan/retention token may already be active",
                span,
            ));
        }
        self.access(
            state,
            &place,
            if kind == LoanKind::Shared {
                ParameterMode::Shared
            } else {
                ParameterMode::Exclusive
            },
            span,
        )?;
        state.loans.insert(
            id,
            Loan {
                places: BTreeSet::from([place]),
                kind,
                definite: true,
                span,
            },
        );
        Ok(())
    }

    fn operation(&self, state: &mut State, op: &Operation, span: Span) -> Check {
        match op {
            Operation::TryCreate(name) => {
                self.sum_destination(state, name, span)?;
                self.initialize(
                    state,
                    name,
                    BTreeSet::from([Origin {
                        parameter: None,
                        projection: vec![],
                    }]),
                );
            }
            Operation::MoveResult { from, to } => {
                self.sum_live(state, from, span)?;
                self.sum_destination(state, to, span)?;
                let origins = self.origins(state, from, span)?;
                self.consume(state, from, span)?;
                self.initialize(state, to, origins);
            }
            Operation::DropResult(from) => {
                self.sum_live(state, from, span)?;
                self.consume(state, from, span)?;
            }

            Operation::OpenRegion(region) => {
                if state.may_open.contains(region) {
                    return Err(error("CGO103", "region may already be open", span));
                }
                state.may_open.insert(region.clone());
                state.must_open.insert(region.clone());
            }
            Operation::CloseRegion(region) => {
                if self.external_regions.contains(region) || !state.must_open.contains(region) {
                    return Err(error(
                        "CGO103",
                        "cannot close caller region or region not definitely open",
                        span,
                    ));
                }
                for local in &self.function.locals {
                    if local.storage == Storage::Region(region.clone())
                        && (state.values[&local.name].may_live
                            || state
                                .loans
                                .values()
                                .any(|loan| loan.places.iter().any(|p| p.local == local.name)))
                    {
                        return Err(error(
                            "CGO103",
                            "region still has possibly live values or loans",
                            span,
                        ));
                    }
                }
                state.must_open.remove(region);
                state.may_open.remove(region);
            }
            Operation::Create(name) => {
                let storage = &self.local(name, span)?.storage;
                self.destination(state, name, storage, span)?;
                self.initialize(
                    state,
                    name,
                    BTreeSet::from([Origin {
                        parameter: None,
                        projection: Vec::new(),
                    }]),
                );
            }
            Operation::Read(place) => self.access(state, place, ParameterMode::Shared, span)?,
            Operation::Write(place) => self.access(state, place, ParameterMode::Exclusive, span)?,
            Operation::Move { from, to } => {
                self.access(state, from, ParameterMode::Owned, span)?;
                self.destination(state, to, &self.local(&from.local, span)?.storage, span)?;
                let origins = self.origins(state, from, span)?;
                self.consume(state, from, span)?;
                self.initialize(state, to, origins);
            }
            Operation::Drop(place) => {
                self.access(state, place, ParameterMode::Owned, span)?;
                self.consume(state, place, span)?;
            }
            Operation::DropRemainder(name) => {
                if !state.values[name].must_live || self.mode(name) != ParameterMode::Owned {
                    return Err(error("CGO101", "no definitely live owned remainder", span));
                }
                self.conflict(state, &Place::local(name), true, span)?;
                self.consume(state, &Place::local(name), span)?;
            }
            Operation::Borrow { place, token, kind } => self.add_loan(
                state,
                LoanId::Local(token.clone()),
                place.clone(),
                *kind,
                span,
            )?,
            Operation::UseLoan { token, write } => {
                let loan = state
                    .loans
                    .get(&LoanId::Local(token.clone()))
                    .filter(|l| l.definite)
                    .ok_or_else(|| error("CGO102", "loan is not definitely active", span))?;
                if *write && loan.kind != LoanKind::Exclusive {
                    return Err(error("CGO102", "cannot write through shared loan", span));
                }
                for place in &loan.places {
                    self.live(state, place, span)?;
                }
            }
            Operation::EndLoan(token) => {
                let id = LoanId::Local(token.clone());
                if !state.loans.get(&id).is_some_and(|l| l.definite) {
                    return Err(error("CGO102", "loan is not definitely active", span));
                }
                state.loans.remove(&id);
            }
            Operation::Call {
                function,
                arguments,
                result,
                retention_owner,
            } => self.call(
                state,
                function,
                arguments,
                result,
                retention_owner.as_deref(),
                span,
            )?,
            Operation::ReleaseForeign(owner) => {
                let loans: Vec<_> = state
                    .loans
                    .iter()
                    .filter(|(id, _)| matches!(id, LoanId::Foreign(name, _) if name == owner))
                    .collect();
                if loans.is_empty() || loans.iter().any(|(_, l)| !l.definite) {
                    return Err(error(
                        "CGO104",
                        "foreign owner is not definitely retained",
                        span,
                    ));
                }
                state
                    .loans
                    .retain(|id, _| !matches!(id, LoanId::Foreign(name, _) if name == owner));
            }
            Operation::Unsupported(_) => unreachable!("preflight rejects unsupported operations"),
        }
        Ok(())
    }

    fn call(
        &self,
        state: &mut State,
        function: &str,
        arguments: &[Place],
        result: &CallResult,
        owner: Option<&str>,
        span: Span,
    ) -> Check {
        let summary = self
            .summaries
            .get(function)
            .ok_or_else(|| error("CGO104", "call has no checked ownership summary", span))?;
        if summary.retention == Retention::Unknown {
            return Err(error(
                "CGO104",
                "unknown foreign retention/lifetime contract",
                span,
            ));
        }
        if arguments.len() != summary.parameters.len() {
            return Err(error(
                "CGO105",
                "call arity differs from ownership summary",
                span,
            ));
        }
        for (i, (place, mode)) in arguments.iter().zip(&summary.parameters).enumerate() {
            self.access(state, place, *mode, span)?;
            for (prior, prior_mode) in arguments[..i].iter().zip(&summary.parameters) {
                if place.overlaps(prior)
                    && (*mode != ParameterMode::Shared || *prior_mode != ParameterMode::Shared)
                {
                    return Err(error("CGO102", "overlapping call arguments conflict", span));
                }
            }
        }
        // Capture provenance before consuming any argument. Binding shapes are
        // checked before publication, and any failure rejects the whole analysis.
        let owned = match (&summary.result, result) {
            (ResultMode::OwnedResult { from_parameter }, CallResult::OwnedResult(to)) => {
                self.sum_destination(state, to, span)?;
                let origins = if let Some(i) = from_parameter {
                    self.origins(state, &arguments[*i], span)?
                } else {
                    BTreeSet::from([Origin {
                        parameter: None,
                        projection: vec![],
                    }])
                };
                Some((to, origins))
            }
            (ResultMode::Owned { from_parameter }, CallResult::Owned(to)) => {
                let (storage, origins) = if let Some(i) = from_parameter {
                    (
                        self.local(&arguments[*i].local, span)?.storage.clone(),
                        self.origins(state, &arguments[*i], span)?,
                    )
                } else {
                    (
                        Storage::Owned,
                        BTreeSet::from([Origin {
                            parameter: None,
                            projection: Vec::new(),
                        }]),
                    )
                };
                self.destination(state, to, &storage, span)?;
                Some((to, origins))
            }
            (ResultMode::Unit, CallResult::Unit)
            | (ResultMode::Loan { .. }, CallResult::Loan(_)) => None,
            _ => {
                return Err(error(
                    "CGO105",
                    "call result binding differs from summary",
                    span,
                ))
            }
        };
        let retained = match (&summary.retention, owner) {
            (Retention::UntilRelease { parameters }, Some(owner)) => {
                if state
                    .loans
                    .keys()
                    .any(|id| matches!(id, LoanId::Foreign(name, _) if name == owner))
                {
                    return Err(error(
                        "CGO104",
                        "foreign owner may already retain arguments",
                        span,
                    ));
                }
                Some((parameters, owner))
            }
            (Retention::UntilRelease { .. }, None) => {
                return Err(error(
                    "CGO104",
                    "retained call requires explicit foreign owner token",
                    span,
                ))
            }
            (_, Some(_)) => return Err(error("CGO105", "unexpected foreign owner token", span)),
            _ => None,
        };
        for (place, mode) in arguments.iter().zip(&summary.parameters) {
            if matches!(mode, ParameterMode::Owned | ParameterMode::OwnedResult) {
                self.consume(state, place, span)?;
            }
        }
        if let Some((to, origins)) = owned {
            self.initialize(state, to, origins);
        }
        if let Some((parameters, owner)) = retained {
            for i in parameters {
                let kind = if summary.parameters[*i] == ParameterMode::Shared {
                    LoanKind::Shared
                } else {
                    LoanKind::Exclusive
                };
                self.add_loan(
                    state,
                    LoanId::Foreign(owner.to_owned(), *i),
                    arguments[*i].clone(),
                    kind,
                    span,
                )?;
            }
        }
        if let (
            ResultMode::Loan {
                parameter,
                projection,
                kind,
            },
            CallResult::Loan(token),
        ) = (&summary.result, result)
        {
            let mut place = arguments[*parameter].clone();
            place.projection.extend(projection.iter().cloned());
            self.path(&place.projection, span)?;
            self.add_loan(state, LoanId::Local(token.clone()), place, *kind, span)?;
        }
        Ok(())
    }

    fn return_value(&self, state: &mut State, value: &ReturnValue, span: Span) -> Check {
        let returned_token = match (&self.function.summary.result, value) {
            (ResultMode::OwnedResult { from_parameter }, ReturnValue::OwnedResult(place)) => {
                self.sum_live(state, place, span)?;
                let origins = self.origins(state, place, span)?;
                if origins.is_empty() || origins.iter().any(|o| &o.parameter != from_parameter) {
                    return Err(error(
                        "CGO103",
                        "conditional result violates Ok provenance",
                        span,
                    ));
                }
                self.consume(state, place, span)?;
                None
            }
            (ResultMode::Unit, ReturnValue::Unit) => None,
            (ResultMode::Owned { from_parameter }, ReturnValue::Owned(place)) => {
                self.access(state, place, ParameterMode::Owned, span)?;
                let origins = self.origins(state, place, span)?;
                if origins.is_empty() || origins.iter().any(|o| &o.parameter != from_parameter) {
                    return Err(error(
                        "CGO103",
                        "owned result does not satisfy parameter provenance contract",
                        span,
                    ));
                }
                if let Storage::Region(region) = &self.local(&place.local, span)?.storage {
                    if !self.external_regions.contains(region) || from_parameter.is_none() {
                        return Err(error("CGO103", "local region value escapes function", span));
                    }
                }
                self.consume(state, place, span)?;
                None
            }
            (
                ResultMode::Loan {
                    parameter,
                    projection,
                    kind,
                },
                ReturnValue::Loan(token),
            ) => {
                let loan = state
                    .loans
                    .get(&LoanId::Local(token.clone()))
                    .filter(|l| l.definite)
                    .ok_or_else(|| error("CGO102", "returned loan is not definitely live", span))?;
                if &loan.kind != kind {
                    return Err(error(
                        "CGO102",
                        "returned loan kind differs from summary",
                        span,
                    ));
                }
                for place in &loan.places {
                    self.live(state, place, span)?;
                    let origins = self.origins(state, place, span)?;
                    if origins.is_empty()
                        || origins
                            .iter()
                            .any(|o| o.parameter != Some(*parameter) || &o.projection != projection)
                    {
                        return Err(error("CGO103", "returned loan escapes a local or mismatches parameter/result relationship", span));
                    }
                }
                Some(token)
            }
            _ => {
                return Err(error(
                    "CGO105",
                    "return shape differs from function summary",
                    span,
                ))
            }
        };
        for (id, loan) in &state.loans {
            if !matches!(id, LoanId::Local(token) if Some(token) == returned_token) {
                let mut diagnostic = error(
                    "CGO102",
                    "active loan or foreign retention escapes function",
                    span,
                );
                diagnostic.related.push(loan.span);
                return Err(diagnostic);
            }
        }
        if state
            .may_open
            .iter()
            .any(|r| !self.external_regions.contains(r))
        {
            return Err(error(
                "CGO103",
                "local region is not closed on every exit",
                span,
            ));
        }
        Ok(())
    }
}
