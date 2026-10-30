# NoFriction Azure static-origin proposal

## Verified now

- Current public route remains `nofriction.io` / `www.nofriction.io` to
  CloudFront `E2AHX31YV2TCE4` and private S3 origin `nofriction-io-site`.
- The independent Azure origin is
  `https://canofrictione242106d.z21.web.core.windows.net/`.
- All 97 objects (18,738,954 bytes) were read from the source, hashed, uploaded
  with their current content headers, read back over HTTPS, and compared. All
  97 body and header checks passed.
- `/`, `/support/index.html`, and the custom missing-object page were tested.
  The clean public `/support/` behavior remains a CloudFront viewer-request
  rewrite, not an Azure object-store behavior.
- No DNS, CloudFront, authentication, application, GCP, or CA21 change was made.

## Publishing path

After this branch lands, `.github/workflows/site_deploy.yml` publishes the
same prepared site to S3/CloudFront and Azure. The Azure job signs in through a
dedicated GitHub OIDC identity bound to the immutable InsureMEP organization,
repository ID, and `main` branch. Its only data role is on this account's
`$web` container. Shared keys and SAS tokens are not used. A failed Azure
publish or origin check fails the workflow, preventing unnoticed S3-only drift.

## Later cutover owned by the migration coordinator

1. Keep the existing CloudFront aliases, certificate, cache policy, response
   headers policy, compression, and `nofriction-io-rewrite` viewer function.
2. Replace the S3/OAC origin with the Azure HTTPS hostname above as a custom
   origin. Use HTTPS-only to origin and preserve the Azure host header.
3. Verify root, clean-directory routes, downloads, media ranges, 301 redirects,
   and custom 404 behavior through the CloudFront distribution hostname.
4. Only after those checks, decide whether any DNS action is needed. No DNS
   change is required merely to switch the existing CloudFront origin.

The exact source manifest, Azure readback, and resource receipt are committed
beside this proposal.
