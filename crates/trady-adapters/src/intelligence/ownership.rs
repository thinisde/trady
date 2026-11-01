//! Bounded ownership XML normalization. No transport, URL resolution or order access.
//!
//! Filing/publication times and amendment targets come from an independently verified
//! EDGAR filing context, never from transaction dates or XML self-assertions.

use chrono::{DateTime, NaiveDate, Utc};
use quick_xml::{Reader, events::Event};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};
use trady_domain::intelligence::insider::*;
use trady_domain::{Error, Result, require};

const MAX_XML_BYTES: usize = 2 * 1024 * 1024;
const MAX_NODES: usize = 50_000;
const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnershipParseContext {
    pub accession: String,
    pub expected_issuer_cik: String,
    pub expected_form: OwnershipForm,
    pub filed_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    /// An archive reference, not a URL to fetch. Only SEC archive references are accepted.
    pub source_ref: String,
    /// XML does not reliably declare the transaction currency; do not infer USD.
    pub transaction_currency: Option<String>,
    pub amendment_of: Option<String>,
    /// New content-derived line id -> exact applicable original filing/line.
    pub amendment_line_targets: BTreeMap<String, InsiderLineRef>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OwnershipFilingParser;

impl OwnershipFilingParser {
    pub fn parse(xml: &str, context: &OwnershipParseContext) -> Result<OwnershipFiling> {
        validate_context(context)?;
        let root = parse_tree(xml)?;
        require(
            root.name == "ownershipDocument",
            "expected SEC ownershipDocument",
        )?;
        require(
            required(&root, &["documentType"])? == context.expected_form.sec_name(),
            "ownership form differs from filing context",
        )?;
        let cik = normalize_cik(&required(&root, &["issuer", "issuerCik"])?)?;
        require(
            cik == normalize_cik(&context.expected_issuer_cik)?,
            "ownership issuer differs from filing context",
        )?;
        let period_of_report = required_date(&root, &["periodOfReport"])?;
        require(
            period_of_report <= context.filed_at.date_naive(),
            "ownership report date follows filing",
        )?;
        let original_submission_date = optional_date(&root, &["dateOfOriginalSubmission"])?;
        if let Some(original) = original_submission_date {
            require(
                original <= context.filed_at.date_naive(),
                "original submission follows amended filing",
            )?;
        }
        let plan_checkbox = optional_bool(&root, &["aff10b5One"])?;
        let mut footnotes = Vec::new();
        if let Some(container) = root.child("footnotes")? {
            let mut ids = BTreeSet::new();
            for node in container.children.iter().filter(|n| n.name == "footnote") {
                let id = node
                    .attributes
                    .get("id")
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| Error("ownership footnote lacks id".into()))?
                    .clone();
                require(
                    ids.insert(id.clone()) && !node.text.trim().is_empty(),
                    "duplicate or empty ownership footnote",
                )?;
                footnotes.push(FilingFootnote {
                    id,
                    text: node.text.trim().to_string(),
                });
            }
        }
        let mut owners = Vec::new();
        let mut seen_owners = BTreeSet::new();
        for node in root.children.iter().filter(|n| n.name == "reportingOwner") {
            let owner_id = normalize_cik(&required(node, &["reportingOwnerId", "rptOwnerCik"])?)?;
            require(
                seen_owners.insert(owner_id.clone()),
                "duplicate reporting owner identity",
            )?;
            let mut missing_reasons = Vec::new();
            let name_as_filed = optional(node, &["reportingOwnerId", "rptOwnerName"])?;
            let is_director = optional_bool(node, &["reportingOwnerRelationship", "isDirector"])?;
            let is_officer = optional_bool(node, &["reportingOwnerRelationship", "isOfficer"])?;
            let is_ten_percent_owner =
                optional_bool(node, &["reportingOwnerRelationship", "isTenPercentOwner"])?;
            let is_other = optional_bool(node, &["reportingOwnerRelationship", "isOther"])?;
            let officer_title = optional(node, &["reportingOwnerRelationship", "officerTitle"])?;
            let other_relationship = optional(node, &["reportingOwnerRelationship", "otherText"])?;
            if name_as_filed.is_none() {
                missing_reasons.push("owner_name_not_disclosed".into());
            }
            if [is_director, is_officer, is_ten_percent_owner, is_other]
                .iter()
                .any(Option::is_none)
            {
                missing_reasons.push("one_or_more_owner_relationship_flags_not_disclosed".into());
            }
            if officer_title.is_none() {
                missing_reasons.push("officer_title_not_disclosed_or_not_applicable".into());
            }
            if other_relationship.is_none() {
                missing_reasons.push("other_relationship_not_disclosed_or_not_applicable".into());
            }
            owners.push(ReportingOwner {
                owner_id,
                name_as_filed,
                is_director,
                is_officer,
                is_ten_percent_owner,
                is_other,
                officer_title,
                other_relationship,
                missing_reasons,
            });
        }
        require(
            !owners.is_empty() && owners.len() <= 256,
            "ownership filing needs bounded reporting owners",
        )?;
        let owner_ids: Vec<_> = seen_owners.into_iter().collect();
        let mut transactions = Vec::new();
        let mut holdings = Vec::new();
        let mut target_ids = BTreeSet::new();
        for (container_name, table, transaction_name, holding_name) in [
            (
                "nonDerivativeTable",
                OwnershipTable::NonDerivative,
                "nonDerivativeTransaction",
                "nonDerivativeHolding",
            ),
            (
                "derivativeTable",
                OwnershipTable::Derivative,
                "derivativeTransaction",
                "derivativeHolding",
            ),
        ] {
            if let Some(container) = root.child(container_name)? {
                for row in &container.children {
                    if row.name == transaction_name {
                        let transaction = transaction(
                            xml,
                            row,
                            table,
                            &owner_ids,
                            &cik,
                            context,
                            plan_checkbox,
                            &footnotes,
                        )?;
                        if transaction.supersedes.is_some() {
                            target_ids.insert(transaction.line_id.clone());
                        }
                        transactions.push(transaction);
                    } else if row.name == holding_name {
                        holdings.push(holding(xml, row, table, &owner_ids, context, &footnotes)?);
                    } else {
                        return Err(Error("unsupported ownership table row".into()));
                    }
                }
            }
        }
        require(
            transactions.len() + holdings.len() <= 10_000,
            "too many ownership rows",
        )?;
        require(
            context
                .amendment_line_targets
                .keys()
                .all(|key| target_ids.contains(key)),
            "amendment context targets an absent new line",
        )?;
        let mut missing_reasons = Vec::new();
        if context.transaction_currency.is_none() {
            missing_reasons.push("transaction_currency_not_supplied_by_verified_context".into());
        }
        if context.expected_form.is_amendment() && context.amendment_of.is_none() {
            missing_reasons.push("amended_accession_not_resolved".into());
        }
        if plan_checkbox.is_none() {
            missing_reasons.push("plan_checkbox_not_present_no_intent_inference".into());
        }
        if transactions.is_empty() {
            missing_reasons
                .push("no_transaction_rows_this_is_not_coverage_of_all_insider_activity".into());
        }
        let issuer_name_as_filed = optional(&root, &["issuer", "issuerName"])?;
        let issuer_trading_symbol = optional(&root, &["issuer", "issuerTradingSymbol"])?;
        if issuer_name_as_filed.is_none() {
            missing_reasons.push("issuer_name_not_disclosed".into());
        }
        if issuer_trading_symbol.is_none() {
            missing_reasons.push("issuer_symbol_not_disclosed".into());
        }
        let remarks = optional(&root, &["remarks"])?;
        Ok(OwnershipFiling {
            issuer_cik: cik,
            issuer_name_as_filed,
            issuer_trading_symbol,
            accession: context.accession.clone(),
            form: context.expected_form,
            filed_at: context.filed_at,
            available_at: context.available_at,
            period_of_report,
            original_submission_date,
            amendment_of: context.amendment_of.clone(),
            plan_checkbox,
            owners,
            transactions,
            holdings,
            footnotes,
            remarks,
            source_ref: context.source_ref.clone(),
            source_content_hash: hash(xml.as_bytes()),
            missing_reasons,
        })
    }
}

fn validate_context(c: &OwnershipParseContext) -> Result<()> {
    require(
        valid_accession(&c.accession) && c.filed_at <= c.available_at,
        "invalid external ownership filing context",
    )?;
    normalize_cik(&c.expected_issuer_cik)?;
    let archive = format!(
        "https://www.sec.gov/Archives/edgar/data/{}/",
        c.expected_issuer_cik.trim_start_matches('0')
    );
    require(
        c.source_ref == format!("sec:{}", c.accession)
            || (c.source_ref.starts_with(&archive)
                && !c.source_ref.contains(['?', '#', '@', '\\'])
                && !c.source_ref.contains("..")
                && !c.source_ref.contains('%')
                && c.source_ref.contains(&c.accession.replace('-', ""))),
        "source reference must identify this SEC archive accession",
    )?;
    if let Some(currency) = &c.transaction_currency {
        require(
            currency.len() == 3 && currency.bytes().all(|b| b.is_ascii_uppercase()),
            "invalid ownership transaction currency",
        )?;
    }
    if let Some(original) = &c.amendment_of {
        require(
            c.expected_form.is_amendment() && valid_accession(original) && original != &c.accession,
            "invalid ownership amendment context",
        )?;
    }
    require(
        c.expected_form.is_amendment() || c.amendment_line_targets.is_empty(),
        "original filing cannot supersede lines",
    )?;
    for (line, target) in &c.amendment_line_targets {
        require(
            !line.is_empty()
                && !target.line_id.is_empty()
                && c.amendment_of.as_ref() == Some(&target.accession),
            "amendment target must identify the applicable external accession",
        )?;
    }
    Ok(())
}

fn valid_accession(s: &str) -> bool {
    s.len() == 20
        && s.bytes().enumerate().all(|(i, b)| {
            if i == 10 || i == 13 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
}

fn normalize_cik(s: &str) -> Result<String> {
    require(
        !s.is_empty()
            && s.len() <= 10
            && s.bytes().all(|b| b.is_ascii_digit())
            && s.bytes().any(|b| b != b'0'),
        "invalid SEC CIK",
    )?;
    Ok(format!("{s:0>10}"))
}

#[derive(Debug)]
struct Node {
    name: String,
    attributes: BTreeMap<String, String>,
    text: String,
    children: Vec<Node>,
    start: u64,
    end: u64,
}

impl Node {
    fn child(&self, name: &str) -> Result<Option<&Node>> {
        let mut children = self.children.iter().filter(|n| n.name == name);
        let first = children.next();
        require(
            children.next().is_none(),
            "duplicate scalar or container in ownership XML",
        )?;
        Ok(first)
    }
    fn path(&self, path: &[&str]) -> Result<Option<&Node>> {
        let mut node = self;
        for name in path {
            let Some(child) = node.child(name)? else {
                return Ok(None);
            };
            node = child;
        }
        Ok(Some(node))
    }
}

fn parse_tree(xml: &str) -> Result<Node> {
    require(
        !xml.is_empty() && xml.len() <= MAX_XML_BYTES && !xml.contains('\0'),
        "ownership XML empty or oversized",
    )?;
    let mut reader = Reader::from_str(xml);
    reader.config_mut().check_end_names = true;
    let mut stack: Vec<Node> = Vec::new();
    let mut root = None;
    let mut nodes = 0_usize;
    let mut declaration_seen = false;
    loop {
        let start = reader.buffer_position();
        let event = reader
            .read_event()
            .map_err(|_| Error("malformed ownership XML".into()))?;
        match event {
            Event::Start(tag) | Event::Empty(tag) => {
                nodes += 1;
                require(
                    nodes <= MAX_NODES && stack.len() < MAX_DEPTH,
                    "ownership XML node or depth limit exceeded",
                )?;
                let name = std::str::from_utf8(tag.name().as_ref())
                    .map_err(|_| Error("non-UTF-8 ownership tag".into()))?
                    .to_string();
                require(
                    !name.is_empty()
                        && name
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
                    "unsupported ownership XML qualified tag",
                )?;
                let mut attributes = BTreeMap::new();
                for attribute in tag.attributes() {
                    let attribute =
                        attribute.map_err(|_| Error("invalid ownership XML attribute".into()))?;
                    let key = std::str::from_utf8(attribute.key.as_ref())
                        .map_err(|_| Error("non-UTF-8 ownership attribute".into()))?
                        .to_string();
                    let value = attribute
                        .unescape_value()
                        .map_err(|_| Error("invalid ownership attribute entity".into()))?
                        .into_owned();
                    require(
                        attributes.insert(key, value).is_none(),
                        "duplicate ownership attribute",
                    )?;
                }
                let node = Node {
                    name,
                    attributes,
                    text: String::new(),
                    children: Vec::new(),
                    start,
                    end: reader.buffer_position(),
                };
                // Empty elements are identified by their original ending, not by their content.
                let end = usize::try_from(reader.buffer_position())
                    .map_err(|_| Error("XML offset overflow".into()))?;
                let is_empty = xml.as_bytes().get(end.saturating_sub(2)..end) == Some(b"/>");
                if is_empty {
                    attach(node, &mut stack, &mut root)?;
                } else {
                    stack.push(node);
                }
            }
            Event::End(_) => {
                let mut node = stack
                    .pop()
                    .ok_or_else(|| Error("unmatched ownership end tag".into()))?;
                node.end = reader.buffer_position();
                attach(node, &mut stack, &mut root)?;
            }
            Event::Text(text) => {
                let text = text
                    .xml_content()
                    .map_err(|_| Error("invalid ownership XML text".into()))?;
                append_text(&mut stack, &text)?;
            }
            Event::CData(text) => {
                let text = text
                    .xml_content()
                    .map_err(|_| Error("invalid ownership XML CDATA".into()))?;
                append_text(&mut stack, &text)?;
            }
            Event::GeneralRef(entity) => {
                let name = std::str::from_utf8(entity.as_ref())
                    .map_err(|_| Error("invalid ownership entity".into()))?;
                let raw = format!("&{name};");
                let text = quick_xml::escape::unescape(&raw)
                    .map_err(|_| Error("unsupported ownership entity".into()))?;
                append_text(&mut stack, &text)?;
            }
            Event::Decl(declaration) => {
                require(
                    !declaration_seen && stack.is_empty() && root.is_none(),
                    "misplaced ownership XML declaration",
                )?;
                declaration_seen = true;
                if let Some(encoding) = declaration.encoding() {
                    require(
                        encoding
                            .map_err(|_| Error("invalid XML encoding declaration".into()))?
                            .eq_ignore_ascii_case(b"UTF-8"),
                        "ownership XML must be UTF-8",
                    )?;
                }
            }
            Event::DocType(_) | Event::PI(_) => {
                return Err(Error(
                    "ownership XML DTD and processing instructions are prohibited".into(),
                ));
            }
            Event::Comment(_) => {}
            Event::Eof => break,
        }
    }
    require(stack.is_empty(), "truncated ownership XML")?;
    root.ok_or_else(|| Error("missing ownership XML root".into()))
}

fn attach(node: Node, stack: &mut [Node], root: &mut Option<Node>) -> Result<()> {
    if let Some(parent) = stack.last_mut() {
        parent.text.push_str(&node.text);
        parent.children.push(node);
    } else {
        require(root.is_none(), "multiple ownership XML roots")?;
        *root = Some(node);
    }
    Ok(())
}

fn append_text(stack: &mut [Node], text: &str) -> Result<()> {
    if let Some(node) = stack.last_mut() {
        node.text.push_str(text);
    } else {
        require(text.trim().is_empty(), "text outside ownership XML root")?;
    }
    Ok(())
}

fn optional(node: &Node, path: &[&str]) -> Result<Option<String>> {
    let Some(value) = node.path(path)? else {
        return Ok(None);
    };
    require(
        value.children.is_empty(),
        "structured content inside ownership scalar",
    )?;
    Ok(Some(value.text.trim().to_string()).filter(|s| !s.is_empty()))
}
fn required(node: &Node, path: &[&str]) -> Result<String> {
    optional(node, path)?
        .ok_or_else(|| Error(format!("missing ownership field {}", path.join("/"))))
}
fn optional_date(node: &Node, path: &[&str]) -> Result<Option<NaiveDate>> {
    optional(node, path)?
        .map(|s| {
            NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                .map_err(|_| Error("invalid ownership date".into()))
        })
        .transpose()
}
fn required_date(node: &Node, path: &[&str]) -> Result<NaiveDate> {
    optional_date(node, path)?
        .ok_or_else(|| Error(format!("missing ownership date {}", path.join("/"))))
}
fn optional_bool(node: &Node, path: &[&str]) -> Result<Option<bool>> {
    optional(node, path)?
        .map(|s| match s.as_str() {
            "1" | "true" => Ok(true),
            "0" | "false" => Ok(false),
            _ => Err(Error("invalid ownership boolean".into())),
        })
        .transpose()
}
fn optional_decimal(node: &Node, path: &[&str]) -> Result<Option<Decimal>> {
    optional(node, path)?
        .map(|s| {
            require(
                s.bytes()
                    .enumerate()
                    .all(|(i, b)| b.is_ascii_digit() || b == b'.' || (i == 0 && b == b'+')),
                "invalid ownership decimal representation",
            )?;
            let value = Decimal::from_str(&s)
                .map_err(|_| Error("ownership decimal overflow or syntax".into()))?;
            require(
                value >= Decimal::ZERO,
                "negative ownership quantity or price",
            )?;
            Ok(value)
        })
        .transpose()
}

fn footnote_ids(row: &Node, notes: &[FilingFootnote]) -> Result<Vec<String>> {
    fn gather(node: &Node, ids: &mut BTreeSet<String>) -> Result<()> {
        if node.name == "footnoteId" {
            let id = node
                .attributes
                .get("id")
                .filter(|s| !s.is_empty())
                .ok_or_else(|| Error("ownership footnote reference lacks id".into()))?;
            ids.insert(id.clone());
        }
        for child in &node.children {
            gather(child, ids)?;
        }
        Ok(())
    }
    let mut ids = BTreeSet::new();
    gather(row, &mut ids)?;
    require(
        ids.iter().all(|id| notes.iter().any(|n| &n.id == id)),
        "dangling ownership footnote reference",
    )?;
    Ok(ids.into_iter().collect())
}

fn plan_annotation(
    checkbox: Option<bool>,
    ids: &[String],
    notes: &[FilingFootnote],
) -> PlanAnnotation {
    let texts: Vec<_> = notes
        .iter()
        .filter(|n| ids.contains(&n.id))
        .map(|n| {
            n.text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase()
        })
        .collect();
    let negated = texts.iter().any(|s| {
        (s.contains("10b5") || s.contains("trading plan"))
            && [
                "not ",
                "no ",
                "neither ",
                "terminated",
                "cancelled",
                "canceled",
            ]
            .iter()
            .any(|n| s.contains(n))
    });
    if negated {
        return PlanAnnotation::Unknown;
    }
    let described = texts
        .iter()
        .any(|s| s.contains("10b5") && s.contains("plan"));
    if checkbox == Some(true) || described {
        PlanAnnotation::DisclosedPlan
    } else {
        PlanAnnotation::Unknown
    }
}

struct Terms {
    direct: Option<String>,
    nature: Option<String>,
    following: Option<Decimal>,
    strike: Option<Decimal>,
    exercise: Option<NaiveDate>,
    expiration: Option<NaiveDate>,
    underlying: Option<String>,
    underlying_shares: Option<Decimal>,
}

fn terms(row: &Node) -> Result<Terms> {
    let direct = optional(
        row,
        &["ownershipNature", "directOrIndirectOwnership", "value"],
    )?;
    require(
        direct.as_deref().is_none_or(|d| d == "D" || d == "I"),
        "invalid direct/indirect ownership",
    )?;
    Ok(Terms {
        direct,
        nature: optional(row, &["ownershipNature", "natureOfOwnership", "value"])?,
        following: optional_decimal(
            row,
            &[
                "postTransactionAmounts",
                "sharesOwnedFollowingTransaction",
                "value",
            ],
        )?,
        strike: optional_decimal(row, &["conversionOrExercisePrice", "value"])?,
        exercise: optional_date(row, &["exerciseDate", "value"])?,
        expiration: optional_date(row, &["expirationDate", "value"])?,
        underlying: optional(
            row,
            &["underlyingSecurity", "underlyingSecurityTitle", "value"],
        )?,
        underlying_shares: optional_decimal(
            row,
            &["underlyingSecurity", "underlyingSecurityShares", "value"],
        )?,
    })
}

#[allow(clippy::too_many_arguments)]
fn transaction(
    xml: &str,
    row: &Node,
    table: OwnershipTable,
    owners: &[String],
    issuer: &str,
    context: &OwnershipParseContext,
    checkbox: Option<bool>,
    notes: &[FilingFootnote],
) -> Result<InsiderTransaction> {
    let security_title = required(row, &["securityTitle", "value"])?;
    let transaction_date = required_date(row, &["transactionDate", "value"])?;
    require(
        transaction_date <= context.filed_at.date_naive(),
        "transaction date follows ownership filing",
    )?;
    let transaction_code = required(row, &["transactionCoding", "transactionCode"])?;
    require(
        transaction_code.len() == 1 && transaction_code.bytes().all(|b| b.is_ascii_uppercase()),
        "invalid SEC transaction code",
    )?;
    let acquired_or_disposed = optional(
        row,
        &[
            "transactionAmounts",
            "transactionAcquiredDisposedCode",
            "value",
        ],
    )?;
    require(
        acquired_or_disposed
            .as_deref()
            .is_none_or(|s| s == "A" || s == "D"),
        "invalid ownership acquisition/disposition code",
    )?;
    let shares = optional_decimal(row, &["transactionAmounts", "transactionShares", "value"])?;
    let price_per_unit = optional_decimal(
        row,
        &["transactionAmounts", "transactionPricePerShare", "value"],
    )?;
    let footnote_ids = footnote_ids(row, notes)?;
    let plan_annotation = plan_annotation(checkbox, &footnote_ids, notes);
    let terms = terms(row)?;
    let line_id = line_hash(xml, row)?;
    let supersedes = context.amendment_line_targets.get(&line_id).cloned();
    let economic_payload = serde_json::to_vec(&(
        (
            issuer,
            owners,
            table,
            &security_title,
            transaction_date,
            &transaction_code,
        ),
        (
            &acquired_or_disposed,
            shares,
            price_per_unit,
            &context.transaction_currency,
        ),
        (
            &terms.direct,
            &terms.nature,
            terms.strike,
            terms.exercise,
            terms.expiration,
            &terms.underlying,
            terms.underlying_shares,
        ),
    ))
    .map_err(|_| Error("cannot serialize ownership identity".into()))?;
    let mut missing_reasons =
        vec!["cash_market_and_footnote_interpretation_requires_explicit_review".into()];
    if shares.is_none() {
        missing_reasons.push("transaction_quantity_not_disclosed".into());
    }
    if price_per_unit.is_none() {
        missing_reasons.push("transaction_price_not_disclosed_footnote_may_describe_range".into());
    }
    if acquired_or_disposed.is_none() {
        missing_reasons.push("acquisition_disposition_not_disclosed".into());
    }
    if context.transaction_currency.is_none() {
        missing_reasons.push("transaction_currency_not_verified".into());
    }
    if terms.direct.is_none() {
        missing_reasons.push("direct_indirect_ownership_not_disclosed".into());
    }
    if terms.nature.is_none() {
        missing_reasons.push("ownership_nature_not_disclosed_or_not_applicable".into());
    }
    if terms.following.is_none() {
        missing_reasons.push("following_holdings_not_disclosed".into());
    }
    if plan_annotation == PlanAnnotation::Unknown {
        missing_reasons.push("plan_status_unknown_no_checkbox_intent_inference".into());
    }
    if context.expected_form.is_amendment() && supersedes.is_none() {
        missing_reasons.push("applicable_amended_transaction_line_not_resolved".into());
    }
    if table == OwnershipTable::Derivative {
        if terms.strike.is_none()
            || terms.exercise.is_none()
            || terms.expiration.is_none()
            || terms.underlying.is_none()
            || terms.underlying_shares.is_none()
        {
            missing_reasons
                .push("one_or_more_derivative_terms_not_disclosed_or_footnote_only".into());
        }
    } else {
        missing_reasons.push("derivative_terms_not_applicable".into());
    }
    Ok(InsiderTransaction {
        economic_id: hash(&economic_payload),
        line_id: line_id.clone(),
        owner_ids: owners.to_vec(),
        table,
        security_title,
        transaction_date,
        transaction_code,
        acquired_or_disposed,
        shares,
        quantity_unit: if table == OwnershipTable::Derivative {
            "derivative_units"
        } else {
            "shares"
        }
        .into(),
        price_per_unit,
        transaction_currency: context.transaction_currency.clone(),
        direct_or_indirect: terms.direct,
        nature_of_ownership: terms.nature,
        shares_owned_following: terms.following,
        conversion_or_exercise_price: terms.strike,
        exercise_date: terms.exercise,
        expiration_date: terms.expiration,
        underlying_security: terms.underlying,
        underlying_shares: terms.underlying_shares,
        footnote_ids,
        plan_annotation,
        cash_market_confirmed: None,
        review_reference: None,
        split_adjustment: None,
        supersedes,
        source_ref: format!("{}#{}", context.source_ref, line_id),
        source_byte_start: row.start,
        source_byte_end: row.end,
        missing_reasons,
    })
}

fn holding(
    xml: &str,
    row: &Node,
    table: OwnershipTable,
    owners: &[String],
    context: &OwnershipParseContext,
    notes: &[FilingFootnote],
) -> Result<InsiderHolding> {
    let terms = terms(row)?;
    let line_id = line_hash(xml, row)?;
    let footnote_ids = footnote_ids(row, notes)?;
    let mut missing_reasons = Vec::new();
    if terms.following.is_none() {
        missing_reasons.push("holding_quantity_not_disclosed_or_footnote_only".into());
    }
    if terms.direct.is_none() {
        missing_reasons.push("direct_indirect_ownership_not_disclosed".into());
    }
    if terms.nature.is_none() {
        missing_reasons.push("ownership_nature_not_disclosed_or_not_applicable".into());
    }
    if table == OwnershipTable::NonDerivative {
        missing_reasons.push("derivative_terms_not_applicable".into());
    } else if terms.strike.is_none()
        || terms.exercise.is_none()
        || terms.expiration.is_none()
        || terms.underlying.is_none()
        || terms.underlying_shares.is_none()
    {
        missing_reasons.push("one_or_more_derivative_terms_not_disclosed_or_footnote_only".into());
    }
    Ok(InsiderHolding {
        line_id: line_id.clone(),
        owner_ids: owners.to_vec(),
        table,
        security_title: required(row, &["securityTitle", "value"])?,
        shares_owned: terms.following,
        direct_or_indirect: terms.direct,
        nature_of_ownership: terms.nature,
        conversion_or_exercise_price: terms.strike,
        exercise_date: terms.exercise,
        expiration_date: terms.expiration,
        underlying_security: terms.underlying,
        underlying_shares: terms.underlying_shares,
        footnote_ids,
        source_ref: format!("{}#{}", context.source_ref, line_id),
        source_byte_start: row.start,
        source_byte_end: row.end,
        missing_reasons,
    })
}

fn line_hash(xml: &str, row: &Node) -> Result<String> {
    let start =
        usize::try_from(row.start).map_err(|_| Error("ownership byte offset overflow".into()))?;
    let end =
        usize::try_from(row.end).map_err(|_| Error("ownership byte offset overflow".into()))?;
    Ok(hash(xml.as_bytes().get(start..end).ok_or_else(|| {
        Error("ownership source byte range invalid".into())
    })?))
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn time() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-30T20:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }
    fn context(form: OwnershipForm) -> OwnershipParseContext {
        OwnershipParseContext {
            accession: "0000000042-26-000001".into(),
            expected_issuer_cik: "42".into(),
            expected_form: form,
            filed_at: time(),
            available_at: time() + Duration::seconds(5),
            source_ref: "sec:0000000042-26-000001".into(),
            transaction_currency: Some("USD".into()),
            amendment_of: None,
            amendment_line_targets: BTreeMap::new(),
        }
    }
    fn document(form: &str, tables: &str, notes: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><ownershipDocument><documentType>{form}</documentType><periodOfReport>2026-10-28</periodOfReport>
            <issuer><issuerCik>42</issuerCik><issuerName>Fixture &amp; Company</issuerName><issuerTradingSymbol>TEST</issuerTradingSymbol></issuer>
            <reportingOwner><reportingOwnerId><rptOwnerCik>7</rptOwnerCik><rptOwnerName>A. Owner</rptOwnerName></reportingOwnerId>
            <reportingOwnerRelationship><isDirector>1</isDirector><isOfficer>true</isOfficer><isTenPercentOwner>0</isTenPercentOwner><isOther>false</isOther><officerTitle>CFO</officerTitle></reportingOwnerRelationship></reportingOwner>
            {tables}{notes}</ownershipDocument>"#
        )
    }
    fn transaction_xml(code: &str) -> String {
        format!(
            r#"<nonDerivativeTransaction><securityTitle><value>Class A Common</value></securityTitle><transactionDate><value>2026-10-28</value></transactionDate>
            <transactionCoding><transactionFormType>4</transactionFormType><transactionCode>{code}</transactionCode></transactionCoding>
            <transactionAmounts><transactionShares><value>500</value></transactionShares><transactionPricePerShare><value>100.25</value><footnoteId id="F1"/></transactionPricePerShare>
            <transactionAcquiredDisposedCode><value>A</value></transactionAcquiredDisposedCode></transactionAmounts>
            <postTransactionAmounts><sharesOwnedFollowingTransaction><value>1000</value></sharesOwnedFollowingTransaction></postTransactionAmounts>
            <ownershipNature><directOrIndirectOwnership><value>I</value></directOrIndirectOwnership><natureOfOwnership><value>Family trust</value></natureOfOwnership></ownershipNature></nonDerivativeTransaction>"#
        )
    }
    fn notes() -> &'static str {
        "<footnotes><footnote id=\"F1\">Weighted average price &amp; fees.</footnote></footnotes>"
    }
    fn form4(code: &str) -> String {
        document(
            "4",
            &format!(
                "<nonDerivativeTable>{}</nonDerivativeTable>",
                transaction_xml(code)
            ),
            notes(),
        )
    }

    #[test]
    fn ownership_normalizes_as_filed_p_s_m_f_without_inventing_intent() {
        for code in ["P", "S", "M", "F"] {
            let xml = form4(code);
            let c = context(OwnershipForm::Form4);
            let f = OwnershipFilingParser::parse(&xml, &c).unwrap();
            assert_eq!(f.issuer_cik, "0000000042");
            assert_eq!(f.issuer_name_as_filed.as_deref(), Some("Fixture & Company"));
            assert_eq!(f.owners[0].officer_title.as_deref(), Some("CFO"));
            let r = &f.transactions[0];
            assert_eq!(r.transaction_code, code);
            assert_eq!(r.shares, Some(Decimal::from(500)));
            assert_eq!(r.price_per_unit, Some(Decimal::new(10025, 2)));
            assert_eq!(r.direct_or_indirect.as_deref(), Some("I"));
            assert_eq!(r.footnote_ids, vec!["F1"]);
            assert_eq!(r.plan_annotation, PlanAnnotation::Unknown);
            assert!(r.cash_market_confirmed.is_none());
            assert_eq!(
                r.line_id,
                hash(
                    xml.as_bytes()
                        .get(r.source_byte_start as usize..r.source_byte_end as usize)
                        .unwrap()
                )
            );
            assert_eq!(f.available_at, c.available_at);
            assert_eq!(f.source_content_hash.len(), 64);
        }
    }

    #[test]
    fn ownership_plan_footnotes_and_false_checkbox_remain_conservative() {
        let xml = form4("S").replace(
            "Weighted average price &amp; fees.",
            "Sale pursuant to a Rule 10b5-1 trading plan.",
        );
        let f = OwnershipFilingParser::parse(&xml, &context(OwnershipForm::Form4)).unwrap();
        assert_eq!(
            f.transactions[0].plan_annotation,
            PlanAnnotation::DisclosedPlan
        );
        let xml = form4("P").replace(
            "<periodOfReport>",
            "<aff10b5One>0</aff10b5One><periodOfReport>",
        );
        assert_eq!(
            OwnershipFilingParser::parse(&xml, &context(OwnershipForm::Form4))
                .unwrap()
                .transactions[0]
                .plan_annotation,
            PlanAnnotation::Unknown
        );
        let xml = form4("P").replace(
            "Weighted average price &amp; fees.",
            "Not pursuant to a Rule 10b5-1 trading plan.",
        );
        assert_eq!(
            OwnershipFilingParser::parse(&xml, &context(OwnershipForm::Form4))
                .unwrap()
                .transactions[0]
                .plan_annotation,
            PlanAnnotation::Unknown
        );
    }

    #[test]
    fn ownership_forms_3_and_5_preserve_holdings_derivatives_and_joint_owners() {
        let hold = "<nonDerivativeHolding><securityTitle><value>Common</value></securityTitle><postTransactionAmounts><sharesOwnedFollowingTransaction><value>500</value></sharesOwnedFollowingTransaction></postTransactionAmounts><ownershipNature><directOrIndirectOwnership><value>D</value></directOrIndirectOwnership></ownershipNature></nonDerivativeHolding>";
        let f = OwnershipFilingParser::parse(
            &document(
                "3",
                &format!("<nonDerivativeTable>{hold}</nonDerivativeTable>"),
                "",
            ),
            &context(OwnershipForm::Form3),
        )
        .unwrap();
        assert!(f.transactions.is_empty());
        assert_eq!(f.holdings[0].shares_owned, Some(Decimal::from(500)));
        let derivative = transaction_xml("M").replace("nonDerivativeTransaction", "derivativeTransaction")
            .replace("<transactionDate>", "<conversionOrExercisePrice><value>10</value></conversionOrExercisePrice><exerciseDate><value>2026-10-28</value></exerciseDate><expirationDate><value>2030-01-01</value></expirationDate><underlyingSecurity><underlyingSecurityTitle><value>Common</value></underlyingSecurityTitle><underlyingSecurityShares><value>500</value></underlyingSecurityShares></underlyingSecurity><transactionDate>");
        let xml = document(
            "5",
            &format!("<derivativeTable>{derivative}</derivativeTable>"),
            notes(),
        );
        let owner = "<reportingOwner><reportingOwnerId><rptOwnerCik>8</rptOwnerCik></reportingOwnerId></reportingOwner>";
        let xml = xml.replace("<derivativeTable>", &format!("{owner}<derivativeTable>"));
        let f = OwnershipFilingParser::parse(&xml, &context(OwnershipForm::Form5)).unwrap();
        assert_eq!(f.transactions[0].table, OwnershipTable::Derivative);
        assert_eq!(f.transactions[0].owner_ids.len(), 2);
        assert_eq!(
            f.transactions[0].conversion_or_exercise_price,
            Some(Decimal::from(10))
        );
        assert_eq!(
            f.transactions[0].underlying_shares,
            Some(Decimal::from(500))
        );
    }

    #[test]
    fn ownership_amendment_links_only_explicitly_supplied_line_and_never_uses_trade_date_as_availability()
     {
        let xml = form4("P").replace(
            "<documentType>4</documentType>",
            "<documentType>4/A</documentType>",
        );
        let mut c = context(OwnershipForm::Form4Amendment);
        c.amendment_of = Some("0000000042-26-000000".into());
        let first = OwnershipFilingParser::parse(&xml, &c).unwrap();
        let id = first.transactions[0].line_id.clone();
        assert!(first.transactions[0].supersedes.is_none());
        c.amendment_line_targets.insert(
            id,
            InsiderLineRef {
                accession: c.amendment_of.clone().unwrap(),
                line_id: "original-content-hash".into(),
            },
        );
        let f = OwnershipFilingParser::parse(&xml, &c).unwrap();
        assert!(f.transactions[0].supersedes.is_some());
        assert!(f.transactions[0].transaction_date < f.available_at.date_naive());
    }

    #[test]
    fn ownership_missing_price_is_explicit_and_economic_duplicates_have_stable_identity() {
        let row = transaction_xml("P");
        let xml = document(
            "4",
            &format!("<nonDerivativeTable>{row}{row}</nonDerivativeTable>"),
            notes(),
        );
        let f = OwnershipFilingParser::parse(&xml, &context(OwnershipForm::Form4)).unwrap();
        assert_eq!(f.transactions[0].economic_id, f.transactions[1].economic_id);
        assert_eq!(f.transactions[0].line_id, f.transactions[1].line_id);
        let xml = form4("P").replace("<value>100.25</value>", "");
        let f = OwnershipFilingParser::parse(&xml, &context(OwnershipForm::Form4)).unwrap();
        assert!(f.transactions[0].price_per_unit.is_none());
        assert!(
            f.transactions[0]
                .missing_reasons
                .iter()
                .any(|r| r.contains("price_not_disclosed"))
        );
    }

    #[test]
    fn ownership_rejects_dtd_entities_untrusted_context_and_ambiguous_scalars() {
        let c = context(OwnershipForm::Form4);
        let dtd = form4("P").replace("<ownershipDocument>", "<!DOCTYPE ownershipDocument [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><ownershipDocument>");
        assert!(OwnershipFilingParser::parse(&dtd, &c).is_err());
        assert!(
            OwnershipFilingParser::parse(&form4("P").replace("Class A Common", "&external;"), &c)
                .is_err()
        );
        assert!(
            OwnershipFilingParser::parse(
                &form4("P").replace("<value>500</value>", "<value>500</value><value>600</value>"),
                &c
            )
            .is_err()
        );
        assert!(
            OwnershipFilingParser::parse(
                &form4("P").replace("id=\"F1\"/>", "id=\"missing\"/>"),
                &c
            )
            .is_err()
        );
        let mut bad = c.clone();
        bad.available_at -= Duration::days(1);
        assert!(OwnershipFilingParser::parse(&form4("P"), &bad).is_err());
        let mut bad = c.clone();
        bad.expected_issuer_cik = "99".into();
        assert!(OwnershipFilingParser::parse(&form4("P"), &bad).is_err());
        let mut bad = c;
        bad.source_ref = "https://attacker.example/filing.xml".into();
        assert!(OwnershipFilingParser::parse(&form4("P"), &bad).is_err());
    }

    #[test]
    fn ownership_parsed_duplicate_rows_reach_policy_as_one_reviewed_economic_event() {
        let row = transaction_xml("P");
        let xml = document(
            "4",
            &format!("<nonDerivativeTable>{row}{row}</nonDerivativeTable>"),
            notes(),
        );
        let mut filing =
            OwnershipFilingParser::parse(&xml, &context(OwnershipForm::Form4)).unwrap();
        for row in &mut filing.transactions {
            row.cash_market_confirmed = Some(true);
            row.plan_annotation = PlanAnnotation::NoPlanDisclosedAfterReview;
            row.review_reference = Some("synthetic:review-of-applicable-footnotes".into());
        }
        let i = InsiderInput {
            issuer_cik: filing.issuer_cik.clone(),
            window_start: time() - Duration::days(3),
            window_end: filing.available_at,
            reporting_currency: "USD".into(),
            minimum_cluster_value: Decimal::from(100),
            minimum_distinct_buyers: 2,
            coverage: FilingCoverage::Partial,
            coverage_reason: Some("synthetic_single_filing_not_complete_owner_coverage".into()),
            filings: vec![filing],
        };
        let result = trady_policy::intelligence::insider::assess(&i, i.window_end).unwrap();
        assert_eq!(result.observed_purchase_value, Some(Decimal::from(50125)));
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.distinct_qualifying_buyers, Some(1));
    }

    #[test]
    fn ownership_rejects_truncation_oversized_depth_and_decimal_overflow() {
        let c = context(OwnershipForm::Form4);
        assert!(
            OwnershipFilingParser::parse(&form4("P").replace("</ownershipDocument>", ""), &c)
                .is_err()
        );
        assert!(
            OwnershipFilingParser::parse(
                &format!("{}x{}", "<x>".repeat(40), "</x>".repeat(40)),
                &c
            )
            .is_err()
        );
        assert!(
            OwnershipFilingParser::parse(
                &form4("P").replace(
                    "<value>500</value>",
                    "<value>99999999999999999999999999999999999999</value>"
                ),
                &c
            )
            .is_err()
        );
    }
}
