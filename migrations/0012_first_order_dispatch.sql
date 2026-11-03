-- First-order intent, approval, reservation, dispatch, and reconciliation state.
--
-- The table is an executor-only extension of the fenced first-order journal. It
-- is deliberately separate from execution.order_intents (the autonomous
-- runtime's order ledger) because a supervised cold start has no order until a
-- protected broker acknowledgement exists. Every mutation is made through a
-- SECURITY DEFINER function; direct table access remains revoked.

CREATE TABLE execution.first_order_approvals (
    account_id text NOT NULL CHECK (account_id ~ '^(paper|shadow|live)_[A-Za-z0-9_-]+$'),
    environment text NOT NULL CHECK (environment IN ('paper','shadow','live')),
    account_day date NOT NULL,
    intent_id text NOT NULL CHECK (intent_id ~ '^[A-Za-z0-9_-]{1,96}$'),
    intent_sha256 text NOT NULL CHECK (intent_sha256 ~ '^[a-f0-9]{64}$'),
    intent_document jsonb NOT NULL CHECK (jsonb_typeof(intent_document) = 'object'),
    owner_id text NOT NULL CHECK (owner_id ~ '^[A-Za-z0-9_-]{1,96}$'),
    approval_sha256 text CHECK (approval_sha256 IS NULL OR approval_sha256 ~ '^[a-f0-9]{64}$'),
    approved_at timestamptz,
    expires_at timestamptz,
    revoked_at timestamptz,
    status text NOT NULL DEFAULT 'PREPARED' CHECK (
        status IN (
            'PREPARED', 'APPROVED', 'RESERVED_FOR_DISPATCH', 'SUBMITTED',
            'RECONCILING', 'RECOVERY_REQUIRED', 'UNKNOWN_SUBMISSION_NO_ID',
            'RECONCILED_COMPLETE'
        )
    ),
    reserved_at timestamptz,
    reservation_expires_at timestamptz,
    dispatch_attempted_at timestamptz,
    dispatch_request_sha256 text CHECK (
        dispatch_request_sha256 IS NULL OR dispatch_request_sha256 ~ '^[a-f0-9]{64}$'
    ),
    broker_order_id text,
    unknown_outcome boolean NOT NULL DEFAULT false,
    reconciliation_document jsonb,
    reconciled_at timestamptz,
    audit_id text NOT NULL CHECK (audit_id ~ '^[A-Za-z0-9_-]{1,96}$'),
    PRIMARY KEY (account_id, environment, account_day),
    UNIQUE (intent_sha256),
    CHECK (
        (status = 'PREPARED' AND approval_sha256 IS NULL AND approved_at IS NULL AND expires_at IS NULL)
        OR (status <> 'PREPARED' AND approval_sha256 IS NOT NULL AND approved_at IS NOT NULL AND expires_at IS NOT NULL)
    ),
    CHECK (expires_at IS NULL OR expires_at > approved_at),
    CHECK (revoked_at IS NULL OR approved_at IS NULL OR revoked_at >= approved_at),
    CHECK (status <> 'SUBMITTED' OR broker_order_id IS NOT NULL),
    CHECK (status <> 'RECONCILED_COMPLETE' OR reconciled_at IS NOT NULL),
    CHECK (reconciliation_document IS NULL OR jsonb_typeof(reconciliation_document) = 'object')
);

REVOKE ALL ON execution.first_order_approvals FROM PUBLIC, trady_analyst, trady_gateway;

CREATE FUNCTION execution.prepare_first_order(
    p_account text,
    p_environment text,
    p_account_day date,
    p_intent_id text,
    p_intent_sha256 text,
    p_owner_id text,
    p_intent_document jsonb,
    p_audit_id text
) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE prior execution.first_order_approvals%ROWTYPE;
BEGIN
    IF coalesce(p_account,'') !~ '^(paper|shadow|live)_[A-Za-z0-9_-]+$'
       OR p_environment NOT IN ('paper','shadow','live')
       OR p_account !~ ('^'||p_environment||'_[A-Za-z0-9_-]+$')
       OR p_account_day IS NULL
       OR coalesce(p_intent_id,'') !~ '^[A-Za-z0-9_-]{1,96}$'
       OR coalesce(p_intent_sha256,'') !~ '^[a-f0-9]{64}$'
       OR coalesce(p_owner_id,'') !~ '^[A-Za-z0-9_-]{1,96}$'
       OR coalesce(p_audit_id,'') !~ '^[A-Za-z0-9_-]{1,96}$'
       OR jsonb_typeof(p_intent_document) IS DISTINCT FROM 'object'
       OR p_intent_document->>'kind' IS DISTINCT FROM 'supervised_first_order_intent'
       OR p_intent_document->>'account_id' IS DISTINCT FROM p_account
       OR p_intent_document->>'intent_id' IS DISTINCT FROM p_intent_id
       THEN RAISE EXCEPTION 'INVALID_FIRST_ORDER_INTENT'; END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended(
        p_account||':'||p_environment||':'||p_account_day::text, 0));
    SELECT * INTO prior
      FROM execution.first_order_approvals
     WHERE account_id=p_account AND environment=p_environment AND account_day=p_account_day
     FOR UPDATE;
    IF FOUND THEN
        IF prior.intent_sha256 IS DISTINCT FROM p_intent_sha256
           OR prior.intent_document IS DISTINCT FROM p_intent_document
           OR prior.intent_id IS DISTINCT FROM p_intent_id
           OR prior.owner_id IS DISTINCT FROM p_owner_id
           THEN RAISE EXCEPTION 'FIRST_ORDER_INTENT_CONFLICT'; END IF;
        RETURN jsonb_build_object(
            'status', prior.status, 'account_id', prior.account_id,
            'intent_id', prior.intent_id, 'intent_sha256', prior.intent_sha256,
            'approval_sha256', prior.approval_sha256
        );
    END IF;
    INSERT INTO execution.first_order_approvals(
        account_id,environment,account_day,intent_id,intent_sha256,intent_document,
        owner_id,audit_id
    ) VALUES (
        p_account,p_environment,p_account_day,p_intent_id,p_intent_sha256,p_intent_document,
        p_owner_id,p_audit_id
    );
    RETURN jsonb_build_object(
        'status','PREPARED', 'account_id',p_account, 'intent_id',p_intent_id,
        'intent_sha256',p_intent_sha256
    );
END $$;

CREATE FUNCTION execution.record_first_order_approval(
    p_account text,
    p_environment text,
    p_account_day date,
    p_intent_id text,
    p_intent_sha256 text,
    p_owner_id text,
    p_approval_sha256 text,
    p_expires_at timestamptz,
    p_audit_id text
) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE prior execution.first_order_approvals%ROWTYPE;
BEGIN
    IF coalesce(p_account,'') !~ '^(paper|shadow|live)_[A-Za-z0-9_-]+$'
       OR p_environment NOT IN ('paper','shadow','live')
       OR p_account !~ ('^'||p_environment||'_[A-Za-z0-9_-]+$')
       OR p_account_day IS NULL
       OR coalesce(p_intent_id,'') !~ '^[A-Za-z0-9_-]{1,96}$'
       OR coalesce(p_intent_sha256,'') !~ '^[a-f0-9]{64}$'
       OR coalesce(p_owner_id,'') !~ '^[A-Za-z0-9_-]{1,96}$'
       OR coalesce(p_approval_sha256,'') !~ '^[a-f0-9]{64}$'
       OR coalesce(p_audit_id,'') !~ '^[A-Za-z0-9_-]{1,96}$'
       OR p_expires_at IS NULL
       OR p_expires_at <= clock_timestamp()
       OR p_expires_at > clock_timestamp()+interval '30 minutes'
       THEN RAISE EXCEPTION 'INVALID_FIRST_ORDER_APPROVAL'; END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended(
        p_account||':'||p_environment||':'||p_account_day::text, 0));
    SELECT * INTO prior
      FROM execution.first_order_approvals
     WHERE account_id=p_account AND environment=p_environment AND account_day=p_account_day
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'FIRST_ORDER_INTENT_NOT_PREPARED';
    END IF;
    IF prior.intent_id IS DISTINCT FROM p_intent_id
       OR prior.intent_sha256 IS DISTINCT FROM p_intent_sha256
       OR prior.owner_id IS DISTINCT FROM p_owner_id
       OR prior.status NOT IN ('PREPARED','APPROVED')
       OR prior.revoked_at IS NOT NULL
       OR (prior.expires_at IS NOT NULL AND prior.expires_at <= clock_timestamp())
       THEN RAISE EXCEPTION 'FIRST_ORDER_APPROVAL_ALREADY_CONSUMED'; END IF;
    IF prior.status='APPROVED' THEN
        IF prior.approval_sha256 IS DISTINCT FROM p_approval_sha256 THEN
            RAISE EXCEPTION 'FIRST_ORDER_APPROVAL_CONFLICT';
        END IF;
        RETURN;
    END IF;
    UPDATE execution.first_order_approvals
       SET approval_sha256=p_approval_sha256,
           approved_at=clock_timestamp(),
           expires_at=p_expires_at,
           status='APPROVED',
           audit_id=p_audit_id
     WHERE account_id=p_account AND environment=p_environment AND account_day=p_account_day;
END $$;

CREATE FUNCTION execution.reserve_first_order(
    p_account text,
    p_environment text,
    p_account_day date,
    p_intent_sha256 text,
    p_approval_sha256 text
) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE prior execution.first_order_approvals%ROWTYPE;
BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended(
        p_account||':'||p_environment||':'||p_account_day::text, 0));
    SELECT * INTO prior
      FROM execution.first_order_approvals
     WHERE account_id=p_account AND environment=p_environment AND account_day=p_account_day
     FOR UPDATE;
    IF NOT FOUND OR prior.intent_sha256 IS DISTINCT FROM p_intent_sha256
       OR prior.approval_sha256 IS DISTINCT FROM p_approval_sha256
       OR prior.status <> 'APPROVED'
       OR prior.revoked_at IS NOT NULL
       OR prior.expires_at <= clock_timestamp()
       THEN RAISE EXCEPTION 'FIRST_ORDER_RESERVATION_UNAVAILABLE'; END IF;
    UPDATE execution.first_order_approvals
       SET status='RESERVED_FOR_DISPATCH', reserved_at=clock_timestamp(),
           reservation_expires_at=clock_timestamp()+interval '30 minutes'
     WHERE account_id=p_account AND environment=p_environment AND account_day=p_account_day;
    RETURN jsonb_build_object(
        'status','RESERVED_FOR_DISPATCH','account_id',p_account,
        'intent_sha256',p_intent_sha256,'approval_sha256',p_approval_sha256,
        'reservation_expires_at',(SELECT reservation_expires_at FROM execution.first_order_approvals
            WHERE account_id=p_account AND environment=p_environment AND account_day=p_account_day)
    );
END $$;

CREATE FUNCTION execution.mark_first_order_dispatch(
    p_account text,
    p_environment text,
    p_account_day date,
    p_intent_sha256 text,
    p_request_sha256 text,
    p_status text,
    p_broker_order_id text
) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE prior execution.first_order_approvals%ROWTYPE;
BEGIN
    IF p_status NOT IN ('SUBMITTED','UNKNOWN_SUBMISSION_NO_ID','RECOVERY_REQUIRED')
       OR coalesce(p_request_sha256,'') !~ '^[a-f0-9]{64}$'
       OR (p_status='SUBMITTED' AND coalesce(p_broker_order_id,'') !~ '^[A-Za-z0-9_-]{1,128}$')
       THEN RAISE EXCEPTION 'INVALID_FIRST_ORDER_DISPATCH'; END IF;
    SELECT * INTO prior
      FROM execution.first_order_approvals
     WHERE account_id=p_account AND environment=p_environment AND account_day=p_account_day
     FOR UPDATE;
    IF NOT FOUND OR prior.intent_sha256 IS DISTINCT FROM p_intent_sha256 THEN
        RAISE EXCEPTION 'FIRST_ORDER_DISPATCH_NOT_FOUND';
    END IF;
    IF prior.status <> 'RESERVED_FOR_DISPATCH' THEN
        IF prior.status IS DISTINCT FROM p_status
           OR prior.dispatch_request_sha256 IS DISTINCT FROM p_request_sha256
           THEN RAISE EXCEPTION 'FIRST_ORDER_DISPATCH_ALREADY_CONSUMED'; END IF;
        RETURN;
    END IF;
    IF prior.reservation_expires_at IS NULL OR prior.reservation_expires_at <= clock_timestamp()
       THEN RAISE EXCEPTION 'FIRST_ORDER_RESERVATION_EXPIRED'; END IF;
    UPDATE execution.first_order_approvals
       SET status=p_status,
           dispatch_attempted_at=clock_timestamp(),
           dispatch_request_sha256=p_request_sha256,
           broker_order_id=p_broker_order_id,
           unknown_outcome=(p_status <> 'SUBMITTED')
     WHERE account_id=p_account AND environment=p_environment AND account_day=p_account_day;
END $$;

CREATE FUNCTION execution.reconcile_first_order(
    p_account text,
    p_environment text,
    p_account_day date,
    p_intent_sha256 text,
    p_broker_order_id text,
    p_status text,
    p_reconciliation_document jsonb
) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE prior execution.first_order_approvals%ROWTYPE;
BEGIN
    IF p_status NOT IN ('RECONCILING','RECOVERY_REQUIRED','RECONCILED_COMPLETE')
       OR jsonb_typeof(p_reconciliation_document) IS DISTINCT FROM 'object'
       THEN RAISE EXCEPTION 'INVALID_FIRST_ORDER_RECONCILIATION'; END IF;
    SELECT * INTO prior
      FROM execution.first_order_approvals
     WHERE account_id=p_account AND environment=p_environment AND account_day=p_account_day
       AND intent_sha256=p_intent_sha256
     FOR UPDATE;
    IF NOT FOUND
       OR prior.status NOT IN ('SUBMITTED','RECONCILING','RECOVERY_REQUIRED','UNKNOWN_SUBMISSION_NO_ID')
       OR prior.dispatch_attempted_at IS NULL
       OR (prior.broker_order_id IS NOT NULL
                     AND prior.broker_order_id IS DISTINCT FROM p_broker_order_id)
       THEN RAISE EXCEPTION 'FIRST_ORDER_RECONCILIATION_MISMATCH'; END IF;
    UPDATE execution.first_order_approvals
       SET status=p_status,
           broker_order_id=coalesce(prior.broker_order_id,p_broker_order_id),
           reconciliation_document=p_reconciliation_document,
           reconciled_at=CASE WHEN p_status='RECONCILED_COMPLETE' THEN clock_timestamp() ELSE reconciled_at END
     WHERE account_id=p_account AND environment=p_environment AND account_day=p_account_day;
END $$;

CREATE FUNCTION execution.revoke_first_order_approval(
    p_account text,
    p_environment text,
    p_account_day date,
    p_intent_sha256 text
) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
BEGIN
    UPDATE execution.first_order_approvals
       SET revoked_at=clock_timestamp(), status='RECOVERY_REQUIRED'
     WHERE account_id=p_account AND environment=p_environment
       AND account_day=p_account_day AND intent_sha256=p_intent_sha256
       AND revoked_at IS NULL AND status IN ('APPROVED','RESERVED_FOR_DISPATCH');
END $$;

CREATE FUNCTION execution.read_first_order_approval(
    p_account text,
    p_environment text,
    p_account_day date,
    p_intent_sha256 text
) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE row_data execution.first_order_approvals%ROWTYPE;
BEGIN
    SELECT * INTO row_data
      FROM execution.first_order_approvals
     WHERE account_id=p_account AND environment=p_environment
       AND account_day=p_account_day AND intent_sha256=p_intent_sha256;
    IF NOT FOUND THEN RETURN NULL; END IF;
    RETURN jsonb_build_object(
        'account_id',row_data.account_id,'environment',row_data.environment,
        'account_day',row_data.account_day,'intent_id',row_data.intent_id,
        'intent_sha256',row_data.intent_sha256,'intent_document',row_data.intent_document,
        'owner_id',row_data.owner_id,'approval_sha256',row_data.approval_sha256,
        'approved_at',row_data.approved_at,'expires_at',row_data.expires_at,
        'revoked_at',row_data.revoked_at,'status',row_data.status,
        'reserved_at',row_data.reserved_at,'reservation_expires_at',row_data.reservation_expires_at,
        'dispatch_attempted_at',row_data.dispatch_attempted_at,
        'dispatch_request_sha256',row_data.dispatch_request_sha256,
        'broker_order_id',row_data.broker_order_id,'unknown_outcome',row_data.unknown_outcome,
        'reconciliation_document',row_data.reconciliation_document,
        'reconciled_at',row_data.reconciled_at,'audit_id',row_data.audit_id
    );
END $$;

REVOKE ALL ON FUNCTION execution.prepare_first_order(
    text,text,date,text,text,text,jsonb,text
), execution.record_first_order_approval(
    text,text,date,text,text,text,text,timestamptz,text
), execution.reserve_first_order(text,text,date,text,text),
execution.mark_first_order_dispatch(text,text,date,text,text,text,text),
execution.reconcile_first_order(text,text,date,text,text,text,jsonb),
execution.revoke_first_order_approval(text,text,date,text),
execution.read_first_order_approval(text,text,date,text)
FROM PUBLIC, trady_analyst, trady_gateway;

GRANT EXECUTE ON FUNCTION execution.prepare_first_order(
    text,text,date,text,text,text,jsonb,text
), execution.record_first_order_approval(
    text,text,date,text,text,text,text,timestamptz,text
), execution.reserve_first_order(text,text,date,text,text),
execution.mark_first_order_dispatch(text,text,date,text,text,text,text),
execution.reconcile_first_order(text,text,date,text,text,text,jsonb),
execution.revoke_first_order_approval(text,text,date,text),
execution.read_first_order_approval(text,text,date,text)
TO trady_executor;
