#!/bin/bash
# Point nofriction.io at the Route 53 nameservers (the one step Claude Code
# can't run for you: it is a registrar change). Run it yourself:
#
#   GD_PAT=gd_pat_... bash infra/site/switch-nameservers.sh
#
# Nothing else changes. Email keeps working: the MX, SPF and autodiscover
# records are already live in the Route 53 zone, identical to GoDaddy's.
# To undo, run with REVERT=1 (puts GoDaddy's own nameservers back).
set -euo pipefail

: "${GD_PAT:?Set GD_PAT to your GoDaddy API token (gd_pat_...)}"
DOMAIN=nofriction.io
API="https://api.godaddy.com/v1/domains/$DOMAIN"

if [ "${REVERT:-0}" = "1" ]; then
  NS='["ns45.domaincontrol.com","ns46.domaincontrol.com"]'
  echo "Reverting $DOMAIN to GoDaddy nameservers…"
else
  NS='["ns-1332.awsdns-38.org","ns-1981.awsdns-55.co.uk","ns-706.awsdns-24.net","ns-496.awsdns-62.com"]'
  echo "Pointing $DOMAIN at Route 53 nameservers…"
fi

code=$(curl -s -o /tmp/switch-ns.out -w '%{http_code}' -X PATCH \
  -H "Authorization: Bearer $GD_PAT" -H "Content-Type: application/json" \
  "$API" -d "{\"nameServers\":$NS}")
if [ "$code" != "200" ] && [ "$code" != "204" ]; then
  echo "GoDaddy returned HTTP $code:"; cat /tmp/switch-ns.out; echo; rm -f /tmp/switch-ns.out; exit 1
fi
rm -f /tmp/switch-ns.out
sleep 3
echo -n "Registrar now reports: "
curl -s -H "Authorization: Bearer $GD_PAT" "$API" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("nameServers"))'
echo "Done. Tell Claude it's switched; propagation takes minutes to an hour."
