# noFriction website source (`site/`)

These existing static HTML/CSS pages preserve the established branding and layout. Their factual copy was updated on 2026-10-03 for Apple on-device or user-configured compatible endpoints only, local transcription, and the approved $0.99/$5.99 eligible one-week trial terms. No website deployment was performed.

The owner identified **nofriction.io**. Its live site belongs to CCA Innovations OU/Casey and publishes `casey@nofriction.io`. Its existing privacy policy describes the consulting website, not the meeting application. The website owner must integrate the app-specific source policy before public app submission, preserving the existing site and its other legal terms.

| Source | Intended integration |
|---|---|
| `index.html` | Meeting-app marketing content; do not overwrite the consulting homepage without the website owner's layout decision |
| `privacy.html` | Meeting-app supplement for `https://nofriction.io/privacy`, or an explicitly agreed app-policy route with both runtime constants updated |
| `support.html` | Meeting-app support material for `https://nofriction.io/contact`; public contact `casey@nofriction.io` |
| `terms.html` | App subscription explanation; runtime Terms link remains Apple's standard EULA |
| `style.css`, `mark.svg` | Existing styles/logo unchanged |

Both apps now use `/privacy`, `/contact` and the published email. Mailbox delivery has not been tested. Relative HTML links support local source preview; production routing must resolve the extensionless runtime URLs and preserve access to the app-specific content. Verify the actual rendered page identity/content and mailbox delivery, not HTTP status alone.

No DNS, hosting, analytics or deployment changes are authorized by this source handoff. The prior generic GitHub Pages instructions were removed because they would replace an existing owned site without establishing its deployment architecture. The website agent should integrate these ready source materials through the site's actual publishing workflow.

Before publication, reconcile this app policy with the final App Privacy declaration and release behavior. A source review or static site preview is not proof of signed-device behavior or live deployment.
