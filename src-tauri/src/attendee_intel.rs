// Attendee Intelligence Module
// Extracts and enriches attendee information from calendar events
// Generates AI-powered briefings on people and companies for meeting prep

use crate::ai_client::AIClient;
use serde::{Deserialize, Serialize};

/// Profile for an individual meeting attendee
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttendeeProfile {
    pub email: String,
    pub name: String,
    pub company: String,
    pub company_domain: String,
    pub briefing: String,
}

/// Profile for a company extracted from attendee emails
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyProfile {
    pub domain: String,
    pub name: String,
    pub briefing: String,
}

/// Complete meeting intelligence package
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingIntelPackage {
    pub event_title: String,
    pub attendees: Vec<AttendeeProfile>,
    pub companies: Vec<CompanyProfile>,
    pub meeting_prep: String,
}

// Common email domains that are not company-specific
const PERSONAL_DOMAINS: &[&str] = &[
    "gmail.com",
    "yahoo.com",
    "hotmail.com",
    "outlook.com",
    "icloud.com",
    "aol.com",
    "protonmail.com",
    "mail.com",
    "live.com",
    "me.com",
    "msn.com",
    "ymail.com",
    "proton.me",
    "fastmail.com",
    "hey.com",
];

/// Extract a display name from an email address
/// e.g. "casey.potenzone@company.com" → "Casey Potenzone"
pub fn extract_name_from_email(email: &str) -> String {
    let local = email.split('@').next().unwrap_or(email);

    // Handle common separators
    let parts: Vec<&str> = local
        .split(|c: char| c == '.' || c == '_' || c == '-')
        .filter(|s| !s.is_empty())
        .collect();

    if parts.is_empty() {
        return email.to_string();
    }

    // Capitalize each part
    parts
        .iter()
        .map(|p| {
            let mut chars = p.chars();
            match chars.next() {
                Some(first) => {
                    let upper: String = first.to_uppercase().collect();
                    format!("{}{}", upper, chars.as_str().to_lowercase())
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract company domain and name from an email address
/// e.g. "casey@acme-corp.com" → ("acme-corp.com", "Acme Corp")
pub fn extract_company_from_email(email: &str) -> (String, String) {
    let domain = email
        .split('@')
        .nth(1)
        .unwrap_or("unknown.com")
        .to_lowercase();

    if PERSONAL_DOMAINS.contains(&domain.as_str()) {
        return (domain.clone(), "Personal".to_string());
    }

    // Extract company name from domain
    let company_part = domain.split('.').next().unwrap_or(&domain);

    // Capitalize and clean up
    let company_name: String = company_part
        .split(|c: char| c == '-' || c == '_')
        .filter(|s| !s.is_empty())
        .map(|p| {
            let mut chars = p.chars();
            match chars.next() {
                Some(first) => {
                    let upper: String = first.to_uppercase().collect();
                    format!("{}{}", upper, chars.as_str().to_lowercase())
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    (domain, company_name)
}

/// Generate an AI-powered briefing on a person
pub async fn generate_person_briefing(
    ai_client: &AIClient,
    name: &str,
    email: &str,
    company: &str,
) -> Result<AttendeeProfile, String> {
    let (domain, company_name) = extract_company_from_email(email);

    let company_display = if company.is_empty() || company == "Personal" {
        company_name.clone()
    } else {
        company.to_string()
    };

    let prompt = format!(
        r#"You are a business intelligence analyst preparing a meeting briefing.

Based on the following information, provide a concise professional profile:

Name: {}
Email: {}
Company: {}

Provide the following in concise markdown (no heading, just content):
1. **Likely Role**: Best guess at their position/title based on name and company context
2. **Company Context**: One-line description of what {} does
3. **Talking Points**: 2-3 suggested topics for conversation
4. **Notes**: Any relevant context (industry trends, company news patterns)

Keep it brief and actionable — this is a quick reference card, not a report.
If you're uncertain about specifics, say "likely" or "estimated" rather than guessing definitively."#,
        name, email, company_display, company_display
    );

    let briefing = match ai_client.complete(&prompt).await {
        Ok(text) => text,
        Err(e) => {
            log::warn!("AI briefing failed for {}: {}", name, e);
            format!(
                "**{}** — {} ({})\n\n*Briefing not available — AI service unavailable*",
                name, company_display, email
            )
        }
    };

    Ok(AttendeeProfile {
        email: email.to_string(),
        name: name.to_string(),
        company: company_display,
        company_domain: domain,
        briefing,
    })
}

/// Generate an AI-powered briefing on a company
pub async fn generate_company_briefing(
    ai_client: &AIClient,
    domain: &str,
    company_name: &str,
) -> Result<CompanyProfile, String> {
    if domain.is_empty() || company_name == "Personal" {
        return Ok(CompanyProfile {
            domain: domain.to_string(),
            name: company_name.to_string(),
            briefing: "*Personal email domain — no company profile available.*".to_string(),
        });
    }

    let prompt = format!(
        r#"You are a business intelligence analyst. Provide a concise company overview.

Company: {}
Domain: {}

Provide in concise markdown (no heading, just content):
1. **Industry**: What sector/industry they operate in
2. **Overview**: 2-3 sentence description of what they do
3. **Size**: Estimated company size (startup, mid-market, enterprise) if inferable
4. **Key Products/Services**: Main offerings
5. **Meeting Context**: Common topics when meeting with people from this type of company

Keep it brief. If uncertain, note it. This is a quick reference, not a research report."#,
        company_name, domain
    );

    let briefing = match ai_client.complete(&prompt).await {
        Ok(text) => text,
        Err(e) => {
            log::warn!("AI briefing failed for company {}: {}", company_name, e);
            format!(
                "**{}** ({})\n\n*Company briefing not available — AI service unavailable*",
                company_name, domain
            )
        }
    };

    Ok(CompanyProfile {
        domain: domain.to_string(),
        name: company_name.to_string(),
        briefing,
    })
}

/// Generate a complete meeting intelligence package from attendee emails
pub async fn generate_meeting_intel(
    ai_client: &AIClient,
    event_title: &str,
    attendee_emails: &[String],
) -> Result<MeetingIntelPackage, String> {
    let mut attendees = Vec::new();
    let mut companies: Vec<CompanyProfile> = Vec::new();
    let mut seen_domains = std::collections::HashSet::new();

    // Generate profiles for each attendee
    for email in attendee_emails {
        let name = extract_name_from_email(email);
        let (domain, company_name) = extract_company_from_email(email);

        // Generate person briefing
        let profile = generate_person_briefing(ai_client, &name, email, &company_name).await?;
        attendees.push(profile);

        // Generate company briefing (once per domain)
        if !seen_domains.contains(&domain) {
            seen_domains.insert(domain.clone());
            let company = generate_company_briefing(ai_client, &domain, &company_name).await?;
            companies.push(company);
        }
    }

    // Generate meeting prep summary
    let attendee_summary: Vec<String> = attendees
        .iter()
        .map(|a| format!("- {} ({}) — {}", a.name, a.company, a.email))
        .collect();

    let company_summary: Vec<String> = companies
        .iter()
        .filter(|c| c.name != "Personal")
        .map(|c| format!("- {} ({})", c.name, c.domain))
        .collect();

    let prep_prompt = format!(
        r#"You are a meeting preparation assistant. Create a concise meeting prep brief.

Meeting: {}

Attendees:
{}

Companies Represented:
{}

Generate a brief meeting prep document in markdown (no top-level heading) including:
1. **Meeting Overview**: One-line purpose estimate based on title and attendees
2. **Key People**: Quick reference for each attendee (name, role, what to discuss)
3. **Conversation Strategy**: 3-4 suggested topics or approaches
4. **Questions to Ask**: 2-3 questions tailored to the attendees and likely meeting purpose

Be concise and actionable."#,
        event_title,
        attendee_summary.join("\n"),
        if company_summary.is_empty() {
            "None identified".to_string()
        } else {
            company_summary.join("\n")
        }
    );

    let meeting_prep = match ai_client.complete(&prep_prompt).await {
        Ok(text) => text,
        Err(e) => {
            log::warn!("Meeting prep generation failed: {}", e);
            format!(
                "**Meeting**: {}\n\n**Attendees**: {}\n\n*Meeting prep not available — AI service unavailable*",
                event_title,
                attendees.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(", ")
            )
        }
    };

    Ok(MeetingIntelPackage {
        event_title: event_title.to_string(),
        attendees,
        companies,
        meeting_prep,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_name_from_email() {
        assert_eq!(
            extract_name_from_email("casey.potenzone@company.com"),
            "Casey Potenzone"
        );
        assert_eq!(
            extract_name_from_email("john_smith@example.com"),
            "John Smith"
        );
        assert_eq!(extract_name_from_email("jane-doe@test.org"), "Jane Doe");
        assert_eq!(extract_name_from_email("admin@company.com"), "Admin");
    }

    #[test]
    fn test_extract_company_from_email() {
        let (domain, name) = extract_company_from_email("casey@acme-corp.com");
        assert_eq!(domain, "acme-corp.com");
        assert_eq!(name, "Acme Corp");

        let (domain, name) = extract_company_from_email("user@gmail.com");
        assert_eq!(domain, "gmail.com");
        assert_eq!(name, "Personal");

        let (domain, name) = extract_company_from_email("user@openai.com");
        assert_eq!(domain, "openai.com");
        assert_eq!(name, "Openai");
    }

    #[test]
    fn test_personal_domain_detection() {
        for domain in &[
            "gmail.com",
            "yahoo.com",
            "hotmail.com",
            "icloud.com",
            "protonmail.com",
        ] {
            let email = format!("user@{}", domain);
            let (_, name) = extract_company_from_email(&email);
            assert_eq!(name, "Personal", "Failed for domain: {}", domain);
        }
    }
}
