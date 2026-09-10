# Agent image publication prerequisites

Product-image qualification is pending. The previous component-root build and
shared-tag smoke instructions do not provide a safe publication procedure for
the monorepo. Do not run that legacy smoke or publish an image based on its
summary. A missing engine now fails; source-only tests remain separate from
actual engine evidence.

Complete the [image artifact and lifecycle requirements](README.md) first:

1. Bind a verified root-context source archive to the exact commit/tree and
   qualify every immutable tool distribution, including Jekko and the auditor's
   actual image-placement evidence. Do not include ambient checkout files or
   credentials in the build context.
2. Build under the existing bounded OCI lifecycle and record the exact image
   digest, source/toolchain inputs, SBOM and dependency/license checks.
3. Run the shared required product-image command on the real engine, preserving
   all 21 existing behaviors and adding the missing identity, offline-build and
   enforcement-cause assertions. Require actual process/container closure and
   retained failure evidence; a probe-image pass is not product-image proof.
4. Obtain the owning publication destination and authority, independent review,
   and protected qualification before publishing or changing runner placement.
   No registry, moving tag or fleet rollout is selected by this document.

The runner's existing JERYU_AGENT_IMAGE interface is unchanged. Setting that
variable does not qualify the referenced image or authorize a fleet rollout.
