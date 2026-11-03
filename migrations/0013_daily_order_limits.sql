-- Enforce the approved daily entry/exit limits in the executor journal.
-- Protective and risk-reducing exit orders are excluded because they preserve
-- the position's risk boundary and are separate from the user's daily exit
-- allowance.

CREATE FUNCTION execution.enforce_daily_order_limits()
RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE
    max_buys integer := coalesce((NEW.config#>>'{policy,max_daily_buy_orders}')::integer, 1);
    max_sells integer := coalesce((NEW.config#>>'{policy,max_daily_sell_orders}')::integer, 1);
    utc_day date := (clock_timestamp() AT TIME ZONE 'UTC')::date;
    buy_count integer;
    sell_count integer;
BEGIN
    IF max_buys NOT BETWEEN 0 AND 1 OR max_sells NOT BETWEEN 0 AND 1 THEN
        RAISE EXCEPTION 'daily order limits exceed one order';
    END IF;
    IF jsonb_typeof(NEW.document->'orders') IS DISTINCT FROM 'array' THEN
        RAISE EXCEPTION 'autonomous order journal is not an array';
    END IF;

    SELECT count(*) INTO buy_count
      FROM jsonb_array_elements(NEW.document->'orders') AS item
     WHERE item->>'role' = 'ENTRY'
       AND item#>>'{request,side}' = 'BUY'
       AND coalesce((item->>'counts_toward_daily_limit')::boolean, true)
       AND ((item->>'created_at')::timestamptz AT TIME ZONE 'UTC')::date = utc_day;
    IF buy_count > max_buys THEN
        RAISE EXCEPTION 'daily buy order limit exceeded';
    END IF;

    SELECT count(*) INTO sell_count
      FROM jsonb_array_elements(NEW.document->'orders') AS item
     WHERE item->>'role' = 'EXIT'
       AND item#>>'{request,side}' = 'SELL'
       AND coalesce((item->>'counts_toward_daily_limit')::boolean, true)
       AND ((item->>'created_at')::timestamptz AT TIME ZONE 'UTC')::date = utc_day;
    IF sell_count > max_sells THEN
        RAISE EXCEPTION 'daily sell order limit exceeded';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER autonomous_daily_order_limits
BEFORE UPDATE OF document ON execution.autonomous_accounts
FOR EACH ROW EXECUTE FUNCTION execution.enforce_daily_order_limits();

REVOKE ALL ON FUNCTION execution.enforce_daily_order_limits() FROM PUBLIC;
