# nofriction.io on S3 + CloudFront

The site is the static files in `site/`. No build step, no framework, no
middleman: `aws s3 sync` to a private bucket, CloudFront in front with an
Origin Access Control, an ACM certificate, and a CloudFront Function that turns
`/support/` into `/support/index.html`. Pushes to `main` that touch `site/**`
deploy through `.github/workflows/site_deploy.yml` once the `AWS_ROLE_ARN`
secret exists; until then the workflow exits 0 with a notice.

Everything below is one-time setup in the AWS console or CLI. Region for the
certificate is **us-east-1** (CloudFront requires it); the bucket can live
anywhere, but us-east-1 keeps it simple.

## 1. S3 bucket (private)

```bash
export AWS_REGION=us-east-1
export BUCKET=nofriction-io-site
aws s3api create-bucket --bucket "$BUCKET" --region us-east-1
aws s3api put-public-access-block --bucket "$BUCKET" \
  --public-access-block-configuration BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true
aws s3api put-bucket-versioning --bucket "$BUCKET" --versioning-configuration Status=Enabled
```

Do not enable "static website hosting" on the bucket; CloudFront reads it as
an S3 origin (REST endpoint), which is what OAC needs.

## 2. ACM certificate (us-east-1)

```bash
aws acm request-certificate --region us-east-1 \
  --domain-name nofriction.io \
  --subject-alternative-names www.nofriction.io \
  --validation-method DNS \
  --query CertificateArn --output text
```

Then `aws acm describe-certificate --certificate-arn <arn> --region us-east-1`
prints two CNAME records (`_xxxx.nofriction.io` and `_xxxx.www.nofriction.io`).
Add them at the DNS provider (step 6). The certificate turns `ISSUED` within
minutes of the records propagating.

## 3. CloudFront Function (URL rewrite)

Source: [`cloudfront-function.js`](cloudfront-function.js). It:

- redirects `www.nofriction.io` to the apex (301),
- redirects `/path` to `/path/` and the old `/support.html`, `/terms.html` to
  their clean URLs (301),
- rewrites `/path/` to `/path/index.html` so S3 finds the object.

```bash
aws cloudfront create-function --name nofriction-io-rewrite \
  --function-config Comment="clean URLs for nofriction.io",Runtime=cloudfront-js-2.0 \
  --function-code fileb://infra/site/cloudfront-function.js
# note the ETag in the output, then publish:
aws cloudfront publish-function --name nofriction-io-rewrite --if-match <ETag>
```

## 4. Origin Access Control + distribution

Create the OAC:

```bash
aws cloudfront create-origin-access-control --origin-access-control-config \
  Name=nofriction-io-oac,SigningProtocol=sigv4,SigningBehavior=always,OriginAccessControlOriginType=s3
```

Create the distribution (console is easiest; these are the settings):

| Setting | Value |
|---|---|
| Origin domain | `nofriction-io-site.s3.us-east-1.amazonaws.com` (the REST endpoint, not the website endpoint) |
| Origin access | Origin access control settings → the OAC above |
| Viewer protocol policy | Redirect HTTP to HTTPS |
| Allowed HTTP methods | GET, HEAD |
| Compress objects automatically | Yes |
| Cache policy | `CachingOptimized` (managed). Objects carry their own `Cache-Control` from `deploy.sh`: a year for `img/`, `video/`, `files/`, badges; 5 minutes for HTML, `style.css`, `js/`, `sitemap.xml`, `robots.txt`. |
| Origin request policy | none |
| Response headers policy | `SecurityHeadersPolicy` (managed), or a custom one adding `Content-Security-Policy: default-src 'self'; img-src 'self' data:; media-src 'self'; script-src 'self'; style-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'` (the site has no inline scripts or styles except `style=""` attributes; if you add the CSP, add `'unsafe-inline'` to `style-src` or move those few inline styles into `style.css`). |
| Function associations | Viewer request → `nofriction-io-rewrite` |
| Alternate domain names (CNAMEs) | `nofriction.io`, `www.nofriction.io` |
| Custom SSL certificate | the ACM certificate from step 2 |
| Security policy | TLSv1.2_2021 |
| Default root object | `index.html` |
| Price class | Use all edge locations (or North America + Europe) |
| HTTP/2 and HTTP/3 | on |

Custom error responses (Error pages tab), so a missing object shows the site's
404 page instead of S3's XML:

| HTTP error code | Response page path | HTTP response code | TTL |
|---|---|---|---|
| 403 | `/404.html` | 404 | 60 |
| 404 | `/404.html` | 404 | 60 |

(S3 returns 403 for a missing key when the bucket is private, so both map to
the same page.)

Bucket policy letting only this distribution read (replace the account id and
distribution id):

```json
{
  "Version": "2012-10-17",
  "Statement": [{
    "Sid": "AllowCloudFrontServicePrincipalReadOnly",
    "Effect": "Allow",
    "Principal": { "Service": "cloudfront.amazonaws.com" },
    "Action": "s3:GetObject",
    "Resource": "arn:aws:s3:::nofriction-io-site/*",
    "Condition": { "StringEquals": { "AWS:SourceArn": "arn:aws:cloudfront::123456789012:distribution/E1ABCDEF2GHIJ" } }
  }]
}
```

```bash
aws s3api put-bucket-policy --bucket "$BUCKET" --policy file://bucket-policy.json
```

## 5. First deploy

```bash
export BUCKET=nofriction-io-site
export DISTRIBUTION_ID=E1ABCDEF2GHIJ
infra/site/deploy.sh
```

`deploy.sh`:
1. copies `marketing/out/hero.mp4` and `hero.webm` into `site/video/` if they
   exist (the film is not committed; the hero shows the poster without it),
2. `aws s3 sync` per file type with the right `Content-Type` and
   `Cache-Control` (long for `img/`, `video/`, `files/`, badges; short for
   HTML, `style.css`, `js/`, `sitemap.xml`, `robots.txt`),
3. a final `aws s3 sync --delete` to remove objects no longer in `site/`
   (`README.md` is never uploaded),
4. `aws cloudfront create-invalidation --paths "/*"`.

`DRY_RUN=1 infra/site/deploy.sh` prints the commands instead of running them.

Check: `curl -sI https://nofriction.io/support/ | head -5` (200, `text/html`),
`curl -sI https://nofriction.io/support` (301 → `/support/`),
`curl -sI https://nofriction.io/nope` (404 with the site's page),
`curl -sI https://www.nofriction.io/` (301 → apex).

## 6. DNS

At the DNS provider for `nofriction.io` (the apex can't be a plain CNAME):

| Name | Type | Value |
|---|---|---|
| `nofriction.io` | ALIAS / ANAME (Route 53: A + AAAA alias to the distribution; Cloudflare: CNAME with flattening, proxy **off**) | `dxxxxxxxxxxxxx.cloudfront.net` |
| `www.nofriction.io` | CNAME | `dxxxxxxxxxxxxx.cloudfront.net` |
| `_xxxx.nofriction.io`, `_xxxx.www.nofriction.io` | CNAME | the two ACM validation records from step 2 |

Route 53 example:

```bash
aws route53 change-resource-record-sets --hosted-zone-id Z0XXXXXXXXXXX --change-batch '{
  "Changes": [
    {"Action":"UPSERT","ResourceRecordSet":{"Name":"nofriction.io","Type":"A","AliasTarget":{"HostedZoneId":"Z2FDTNDATAQYW2","DNSName":"dxxxxxxxxxxxxx.cloudfront.net","EvaluateTargetHealth":false}}},
    {"Action":"UPSERT","ResourceRecordSet":{"Name":"nofriction.io","Type":"AAAA","AliasTarget":{"HostedZoneId":"Z2FDTNDATAQYW2","DNSName":"dxxxxxxxxxxxxx.cloudfront.net","EvaluateTargetHealth":false}}},
    {"Action":"UPSERT","ResourceRecordSet":{"Name":"www.nofriction.io","Type":"CNAME","TTL":300,"ResourceRecords":[{"Value":"dxxxxxxxxxxxxx.cloudfront.net"}]}}
  ]}'
```

(`Z2FDTNDATAQYW2` is the fixed hosted-zone id for every CloudFront
distribution.) Keep the existing MX records for `casey@nofriction.io`
untouched. The current site is hosted by Lovable; switching the apex and `www`
records moves traffic, so do the first deploy (step 5) and the `curl` checks
against the `*.cloudfront.net` name before changing DNS.

## 7. GitHub Actions deploy (OIDC, no long-lived keys)

`.github/workflows/site_deploy.yml` runs on every push to `main` that touches
`site/**`, `infra/site/**` or the workflow itself, and on `workflow_dispatch`.
It assumes an IAM role through OIDC and runs `deploy.sh`. It does nothing
(exit 0 with a notice) until these repository secrets exist:

| Secret | Value |
|---|---|
| `AWS_ROLE_ARN` | `arn:aws:iam::123456789012:role/nofriction-io-site-deploy` |
| `SITE_BUCKET` | `nofriction-io-site` |
| `SITE_DISTRIBUTION_ID` | `E1ABCDEF2GHIJ` |

```bash
gh secret set AWS_ROLE_ARN -R Cpotenzone/nofriction-meetings
gh secret set SITE_BUCKET -R Cpotenzone/nofriction-meetings
gh secret set SITE_DISTRIBUTION_ID -R Cpotenzone/nofriction-meetings
```

### OIDC provider (once per account)

```bash
aws iam create-open-id-connect-provider \
  --url https://token.actions.githubusercontent.com \
  --client-id-list sts.amazonaws.com
```

### Role trust policy

Only the `main` branch of this repository can assume the role:

```json
{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Principal": { "Federated": "arn:aws:iam::123456789012:oidc-provider/token.actions.githubusercontent.com" },
    "Action": "sts:AssumeRoleWithWebIdentity",
    "Condition": {
      "StringEquals": { "token.actions.githubusercontent.com:aud": "sts.amazonaws.com" },
      "StringLike": { "token.actions.githubusercontent.com:sub": "repo:Cpotenzone/nofriction-meetings:ref:refs/heads/main" }
    }
  }]
}
```

### Role permissions policy

```json
{
  "Version": "2012-10-17",
  "Statement": [
    { "Effect": "Allow", "Action": ["s3:ListBucket"], "Resource": "arn:aws:s3:::nofriction-io-site" },
    { "Effect": "Allow", "Action": ["s3:PutObject", "s3:DeleteObject", "s3:GetObject"], "Resource": "arn:aws:s3:::nofriction-io-site/*" },
    { "Effect": "Allow", "Action": ["cloudfront:CreateInvalidation"], "Resource": "arn:aws:cloudfront::123456789012:distribution/E1ABCDEF2GHIJ" }
  ]
}
```

```bash
aws iam create-role --role-name nofriction-io-site-deploy --assume-role-policy-document file://trust.json
aws iam put-role-policy --role-name nofriction-io-site-deploy --policy-name deploy --policy-document file://permissions.json
```

## 8. Flipping the site to "live" on launch day

One edit: `site/js/store.js`, `live: false` → `live: true`. Every "Get
notified" block becomes Apple's App Store and Mac App Store badges and the
header button says "Download". Commit to `main`; the workflow deploys it.

## Costs and notes

- S3 + CloudFront for a site this size (about 2.5 MB of assets, HTML under
  40 KB a page) sits inside the free tier or a few cents a month.
- The hero film (`video/hero.mp4` and `.webm`) is the only large object;
  keep it under about 5 MB each or lazy-load it.
- No trackers, no cookies, no analytics scripts on the site. CloudFront
  standard logging can be turned on to an S3 bucket if traffic numbers are
  ever needed; the website privacy policy already covers server access logs.
