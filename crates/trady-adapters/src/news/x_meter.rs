//! Reserve each X request durably, independently of page/cursor persistence.
use super::*;
use trady_application::x_monitoring::*;

pub struct MeteredHttp<T, R> {
    inner: T,
    repository: R,
    policy: Option<XBudgetPolicy>,
    scope: Option<XCostScope>,
}

impl<T, R> MeteredHttp<T, R> {
    pub fn new(inner: T, repository: R, policy: Option<XBudgetPolicy>) -> Self {
        Self {
            inner,
            repository,
            policy,
            scope: None,
        }
    }
}

fn failure(_: Error) -> SourceFailure {
    SourceFailure::unavailable("x_cost_ledger_unavailable_or_policy_changed")
}

#[derive(Clone, Copy)]
enum Endpoint {
    User,
    Counts,
    Posts { lookup: bool },
}

fn bound(url: &Url) -> std::result::Result<(Endpoint, XCostPlan), SourceFailure> {
    let fail = || SourceFailure::unavailable("x_unreviewed_resource_request");
    let path = url.path();
    let mut plan = XCostPlan::default();
    let endpoint = if path.starts_with("/2/users/by/username/") {
        let handle = path
            .strip_prefix("/2/users/by/username/")
            .ok_or_else(fail)?;
        if url.query().is_some()
            || handle.is_empty()
            || handle.len() > 15
            || !handle
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(fail());
        }
        plan.support.users = 1;
        Endpoint::User
    } else if path == "/2/tweets/counts/recent" {
        plan.support.counts = 1;
        Endpoint::Counts
    } else {
        let lookup = path.strip_prefix("/2/tweets/").is_some_and(valid_x_id);
        if !lookup
            && path != "/2/tweets/search/recent"
            && !(path.starts_with("/2/users/") && path.ends_with("/tweets"))
        {
            return Err(fail());
        }
        let count = if lookup {
            1
        } else {
            url.query_pairs()
                .find(|(k, _)| k == "max_results")
                .and_then(|(_, v)| v.parse::<u32>().ok())
                .filter(|n| (1..=100).contains(n))
                .ok_or_else(fail)?
        };
        let expanded = url.query_pairs().any(|(k, _)| k == "expansions");
        if lookup {
            plan.support.posts = count;
        } else {
            plan.ordinary.posts = count;
        }
        if expanded {
            // Three references and six edit IDs per returned post, plus all their authors.
            let context = url
                .query_pairs()
                .filter(|(k, _)| k == "expansions")
                .any(|(_, v)| v.split(',').any(|p| p != "author_id"));
            plan.support.posts += if context { count * 9 } else { 0 };
            plan.support.users = if context { count * 10 } else { count };
        }
        Endpoint::Posts { lookup }
    };
    Ok((endpoint, plan))
}

fn consumption(endpoint: Endpoint, response: &HttpResponse) -> Option<XCostPlan> {
    if response.status != 200 {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&response.body).ok()?;
    if value
        .get("errors")
        .is_some_and(|e| e.as_array().is_none_or(|a| !a.is_empty()))
    {
        return None;
    }
    let mut plan = XCostPlan::default();
    match endpoint {
        Endpoint::User => {
            value.get("data")?.as_object()?;
            plan.support.users = 1;
        }
        Endpoint::Counts => plan.support.counts = 1,
        Endpoint::Posts { lookup } => {
            let count = match value.get("data") {
                Some(data) if lookup => {
                    data.as_object()?;
                    1
                }
                Some(data) => u32::try_from(data.as_array()?.len()).ok()?,
                None if value.pointer("/meta/result_count").and_then(|n| n.as_u64()) == Some(0) => {
                    0
                }
                _ => return None,
            };
            if lookup {
                plan.support.posts = count;
            } else {
                plan.ordinary.posts = count;
            }
        }
    }
    if let Some(includes) = value.get("includes") {
        for (kind, values) in includes.as_object()? {
            let count = u32::try_from(values.as_array()?.len()).ok()?;
            match kind.as_str() {
                "posts" | "tweets" => plan.support.posts = plan.support.posts.checked_add(count)?,
                "users" => plan.support.users = plan.support.users.checked_add(count)?,
                _ if count == 0 => {}
                _ => return None,
            }
        }
    }
    Some(plan)
}

impl<T: HttpTransport, R: XCostRepository> HttpTransport for MeteredHttp<T, R> {
    fn x_budget_enforced(&self) -> bool {
        true
    }

    fn set_x_scope(&mut self, source: &SourceConfig) -> std::result::Result<(), SourceFailure> {
        self.scope = None;
        if matches!(
            source.kind,
            SourceKind::XTimeline | SourceKind::XLookup | SourceKind::XRecentSearch
        ) {
            let rule_hash = match (&source.x_rule, &source.x_account) {
                (Some(rule), Some(account)) => Some(
                    rule.hash(
                        &account.handle,
                        account.user_id.as_deref().unwrap_or_default(),
                    )
                    .map_err(failure)?,
                ),
                _ => None,
            };
            self.scope = Some(XCostScope {
                source_id: source.id.clone(),
                registry_hash: fingerprint(source).map_err(failure)?,
                rule_hash,
            });
        }
        Ok(())
    }

    fn get(&mut self, request: HttpRequest) -> std::result::Result<HttpResponse, SourceFailure> {
        if request.url.host_str() != Some("api.x.com") {
            return self.inner.get(request);
        }
        let now = Utc::now();
        let policy = self
            .policy
            .as_ref()
            .ok_or_else(|| SourceFailure::unavailable("x_billing_policy_required"))?;
        policy
            .active(now)
            .map_err(|_| SourceFailure::unavailable("x_billing_policy_disabled_or_expired"))?;
        let scope = self
            .scope
            .as_ref()
            .ok_or_else(|| SourceFailure::unavailable("x_cost_scope_required"))?;
        let (endpoint, plan) = bound(&request.url)?;
        let reservation = self
            .repository
            .reserve(policy, scope, plan, now)
            .map_err(failure)?
            .ok_or_else(|| SourceFailure {
                status: CollectionStatus::RateLimited,
                reason: "x_resource_cost_budget_exhausted".into(),
                retry_at: Some(policy.cycle_end),
            })?;
        let response = self.inner.get(request);
        let actual = response
            .as_ref()
            .ok()
            .and_then(|r| consumption(endpoint, r));
        let exceeded = actual.is_some_and(|a| {
            a.ordinary.posts > plan.ordinary.posts
                || a.support.posts > plan.support.posts
                || a.support.users > plan.support.users
                || a.support.counts > plan.support.counts
        });
        let outcome = if exceeded {
            "RESOURCE_BOUND_EXCEEDED"
        } else if actual.is_some() {
            "OBSERVED"
        } else {
            "UNKNOWN_CONSUMPTION"
        };
        self.repository
            .settle(reservation, actual, outcome)
            .map_err(failure)?;
        if exceeded {
            return Err(SourceFailure::unavailable(
                "x_provider_exceeded_resource_bound",
            ));
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{cell::RefCell, rc::Rc};

    #[derive(Default)]
    struct Events {
        order: Vec<&'static str>,
        denied: bool,
        plans: Vec<XCostPlan>,
        settlements: Vec<(Option<XCostPlan>, String)>,
    }
    struct Ledger(Rc<RefCell<Events>>);
    impl XCostRepository for Ledger {
        fn reserve(
            &mut self,
            _: &XBudgetPolicy,
            _: &XCostScope,
            plan: XCostPlan,
            _: DateTime<Utc>,
        ) -> Result<Option<i64>> {
            let mut events = self.0.borrow_mut();
            events.order.push("reserve");
            events.plans.push(plan);
            Ok(if events.denied { None } else { Some(1) })
        }
        fn settle(&mut self, _: i64, actual: Option<XCostPlan>, outcome: &str) -> Result<()> {
            let mut events = self.0.borrow_mut();
            events.order.push("settle");
            events.settlements.push((actual, outcome.into()));
            Ok(())
        }
    }
    struct Network {
        events: Rc<RefCell<Events>>,
        response: Option<std::result::Result<HttpResponse, SourceFailure>>,
    }
    impl HttpTransport for Network {
        fn get(&mut self, _: HttpRequest) -> std::result::Result<HttpResponse, SourceFailure> {
            self.events.borrow_mut().order.push("network");
            self.response.take().unwrap()
        }
    }
    fn fixture(
        response: std::result::Result<HttpResponse, SourceFailure>,
    ) -> (MeteredHttp<Network, Ledger>, Rc<RefCell<Events>>) {
        let events = Rc::new(RefCell::new(Events::default()));
        let cfg: ResearchConfig =
            serde_json::from_slice(include_bytes!("../../../../config/x-pilot.example.json"))
                .unwrap();
        let mut policy = cfg.x_budget.unwrap();
        policy.enabled = true;
        policy.cycle_start = Utc::now() - Duration::hours(1);
        policy.cycle_end = Utc::now() + Duration::days(29);
        policy.approval_reference = "fixture owner".into();
        policy.entitlement_reference = "fixture entitlement".into();
        policy.provider_limit_reference = "fixture spending limit".into();
        let mut transport = MeteredHttp::new(
            Network {
                events: events.clone(),
                response: Some(response),
            },
            Ledger(events.clone()),
            Some(policy),
        );
        transport.set_x_scope(&cfg.sources[0]).unwrap();
        (transport, events)
    }
    fn request(path: &str) -> HttpRequest {
        HttpRequest {
            url: Url::parse(&format!("https://api.x.com/2/{path}")).unwrap(),
            allowed_hosts: vec!["api.x.com".into()],
            bearer: Some("fixture".into()),
            etag: None,
            last_modified: None,
            redirects: 0,
        }
    }
    fn response(body: serde_json::Value) -> std::result::Result<HttpResponse, SourceFailure> {
        Ok(HttpResponse {
            status: 200,
            body: serde_json::to_vec(&body).unwrap(),
            ..Default::default()
        })
    }

    #[test]
    fn reserves_before_network_and_accounts_for_returned_posts_and_author_users() {
        let (mut transport, events) = fixture(response(
            json!({"data":[{"id":"1"},{"id":"2"}],"includes":{"users":[{"id":"42"}]}}),
        ));
        transport
            .get(request(
                "tweets/search/recent?max_results=10&expansions=author_id",
            ))
            .unwrap();
        let events = events.borrow();
        assert_eq!(events.order, vec!["reserve", "network", "settle"]);
        assert_eq!(events.plans[0].ordinary.posts, 10);
        assert_eq!(events.plans[0].support.users, 10);
        assert_eq!(events.plans[0].support.posts, 0);
        let actual = events.settlements[0].0.unwrap();
        assert_eq!(actual.ordinary.posts, 2);
        assert_eq!(actual.support.users, 1);
    }

    #[test]
    fn budget_denial_or_missing_policy_prevents_network() {
        let (mut transport, events) = fixture(response(json!({"data":{"id":"42"}})));
        events.borrow_mut().denied = true;
        assert_eq!(
            transport
                .get(request("users/by/username/fixture"))
                .err()
                .unwrap()
                .reason,
            "x_resource_cost_budget_exhausted"
        );
        assert_eq!(events.borrow().order, vec!["reserve"]);
        transport.policy = None;
        assert_eq!(
            transport
                .get(request("users/by/username/fixture"))
                .err()
                .unwrap()
                .reason,
            "x_billing_policy_required"
        );
        assert_eq!(events.borrow().order, vec!["reserve"]);
    }

    #[test]
    fn failed_or_malformed_reads_never_refund_uncertain_consumption() {
        for response in [
            Err(SourceFailure::unavailable("outage")),
            Ok(HttpResponse {
                status: 200,
                body: b"malformed".to_vec(),
                ..Default::default()
            }),
            Ok(HttpResponse {
                status: 402,
                ..Default::default()
            }),
        ] {
            let (mut transport, events) = fixture(response);
            let _ = transport.get(request("users/by/username/fixture"));
            assert_eq!(events.borrow().order, vec!["reserve", "network", "settle"]);
            assert_eq!(
                events.borrow().settlements[0],
                (None, "UNKNOWN_CONSUMPTION".into())
            );
        }
    }

    #[test]
    fn unexpected_resource_overrun_is_recorded_and_stops_delivery() {
        let (mut transport, events) = fixture(response(json!({"data":[{"id":"1"},{"id":"2"}]})));
        assert_eq!(
            transport
                .get(request("tweets/search/recent?max_results=1"))
                .err()
                .unwrap()
                .reason,
            "x_provider_exceeded_resource_bound"
        );
        assert_eq!(events.borrow().settlements[0].1, "RESOURCE_BOUND_EXCEEDED");
        assert_eq!(events.borrow().settlements[0].0.unwrap().ordinary.posts, 2);
    }

    #[test]
    fn identity_queries_cannot_add_unbudgeted_expansions_and_unexpected_resources_are_counted() {
        let (mut transport, events) = fixture(response(json!({"data":{"id":"42"}})));
        assert_eq!(
            transport
                .get(request(
                    "users/by/username/fixture?expansions=pinned_post_id"
                ))
                .err()
                .unwrap()
                .reason,
            "x_unreviewed_resource_request"
        );
        assert!(events.borrow().order.is_empty());
        let (mut transport, events) = fixture(response(
            json!({"data":{"id":"42"},"includes":{"posts":[{"id":"1"}]}}),
        ));
        assert_eq!(
            transport
                .get(request("users/by/username/fixture"))
                .err()
                .unwrap()
                .reason,
            "x_provider_exceeded_resource_bound"
        );
        assert_eq!(events.borrow().settlements[0].0.unwrap().support.posts, 1);
    }
}
