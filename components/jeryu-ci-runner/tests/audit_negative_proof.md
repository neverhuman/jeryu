# Tenant Isolation Negative Proof

This test witness covers runner and cache-policy surfaces that carry `tenant_id`.

Required negative proof:

- tenant isolation rejects a wrong user claiming another tenant runner lease
- tenant isolation rejects a non-owner completing another tenant workcell job
- forbidden cross-tenant scheduling remains covered by the runner workcell tests
