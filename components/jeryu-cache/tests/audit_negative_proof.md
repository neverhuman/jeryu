# Tenant Isolation Negative Proof

This test witness covers cache surfaces that carry `tenant_id`.

Required negative proof:

- tenant isolation rejects a wrong user reading another tenant cache key
- tenant isolation rejects a non-owner writing another tenant cache namespace
- forbidden cross-tenant lookups remain covered by `tests/cache_poisoning_matrix.sh`
