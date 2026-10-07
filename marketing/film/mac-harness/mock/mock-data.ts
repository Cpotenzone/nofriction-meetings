// Fictional demo data for the film harness. Every name, meeting, lecture,
// number and link here is invented; links use example.com only.
//
// Times are built from the harness clock (see time.ts: the page's "now" is
// a weekday morning), so the library reads like a real week.

export type Kind = "meeting" | "class" | "personal";

export interface DemoLine {
    at: string; // "m:ss" from the recording start
    speaker: string;
    text: string;
}

export interface DemoFrame {
    at: string; // "m:ss"
    image: string; // frames/<image>.jpg (make-frames.mjs)
}

export interface DemoMarker {
    at: string;
    kind: "important" | "question" | "test";
    note: string | null;
}

// Links are only "said" or "added": the Mac App Store build reads no
// screen text, so it never finds links "on screen".
export interface DemoMeeting {
    id: string;
    title: string;
    kind: Kind;
    notebook: string | null;
    /** Days before today, and the local start time "HH:MM" */
    daysAgo: number;
    startTime: string;
    durationMin: number;
    plannedMinutes: number | null;
    lines?: DemoLine[];
    frames?: DemoFrame[];
    markers?: DemoMarker[];
    notes?: {
        model_used: string | null;
        summary: string;
        key_topics: string[];
        decisions: { text: string; made_by?: string | null; context?: string | null }[];
        action_items: { task: string; assignee?: string | null; due_date?: string | null }[];
    };
    links?: {
        url: string;
        title?: string | null;
        note?: string | null;
        sources: ("added" | "said" | "screen")[];
        said?: number;
        screen?: number;
        first?: string | null; // "m:ss"
        firstSource?: "said" | "screen" | null;
    }[];
    study?: Record<string, unknown>;
}

// ── Acme project sync (Meeting · Acme project) ─────────────────────────

const acme: DemoMeeting = {
    id: "m-acme-sync",
    title: "Acme project sync",
    kind: "meeting",
    notebook: "Acme project",
    daysAgo: 0,
    startTime: "09:00",
    durationMin: 19.4,
    plannedMinutes: 30,
    frames: [
        { at: "0:10", image: "acme-1" },
        { at: "1:46", image: "acme-2" },
        { at: "3:34", image: "acme-2b" },
        { at: "4:36", image: "acme-3" },
        { at: "7:16", image: "acme-4" },
        { at: "10:26", image: "acme-5" },
        { at: "14:07", image: "acme-6" },
        { at: "17:40", image: "acme-7" },
    ],
    lines: [
        { at: "0:08", speaker: "Priya", text: "Okay, let's get started. Quick agenda: the roadmap, beta feedback, onboarding, and the launch checklist." },
        { at: "0:31", speaker: "Marcus", text: "Sounds good. I have a short update on billing too." },
        { at: "1:44", speaker: "Priya", text: "Here's the Q4 roadmap. The private beta wraps up on the twenty-fourth." },
        { at: "2:15", speaker: "Dana", text: "Onboarding starts next week, and it overlaps billing by about two weeks." },
        { at: "2:52", speaker: "Marcus", text: "That overlap is fine if we lock the plan names by Friday." },
        { at: "3:30", speaker: "Priya", text: "Let's do that. Plan names are locked by Friday." },
        { at: "4:34", speaker: "Priya", text: "Sign-ups keep climbing. Six hundred forty last week." },
        { at: "5:05", speaker: "Marcus", text: "That's up twenty-four percent week over week." },
        { at: "5:48", speaker: "Dana", text: "Most of them came in through the public roadmap. It's at example.com/roadmap if you want to look." },
        { at: "7:14", speaker: "Priya", text: "Here's what the beta teams told us." },
        { at: "7:52", speaker: "Dana", text: "Setup taking too long is the biggest complaint. That's exactly what the new onboarding fixes." },
        { at: "8:40", speaker: "Marcus", text: "Could we send the invites after the first project instead of during setup?" },
        { at: "9:20", speaker: "Dana", text: "Yes. I'll mock that up." },
        { at: "10:24", speaker: "Dana", text: "So this is the new flow. Three steps: workspace, first project, then the team." },
        { at: "11:10", speaker: "Priya", text: "I like it. Can we test it with five beta teams before we build it?" },
        { at: "11:48", speaker: "Dana", text: "I can run those sessions next week." },
        { at: "14:05", speaker: "Priya", text: "Launch checklist. Pricing copy and the feedback survey are done." },
        { at: "14:40", speaker: "Marcus", text: "I'll take the status page. The help center articles are still open." },
        { at: "15:22", speaker: "Priya", text: "Dana, can you draft the launch email?" },
        { at: "15:50", speaker: "Dana", text: "Sure. I'll have a draft by the twentieth." },
        { at: "17:34", speaker: "Marcus", text: "Last thing: billing needs a load test before launch. The plan is at example.com/acme/load-test." },
        { at: "18:10", speaker: "Priya", text: "Let's add that to the risks and check in next Tuesday." },
        { at: "18:42", speaker: "Priya", text: "Thanks, everyone. Good meeting." },
    ],
    markers: [
        { at: "3:32", kind: "important", note: "Plan names locked by Friday" },
        { at: "8:42", kind: "question", note: "Invites after the first project?" },
        { at: "15:51", kind: "test", note: "Launch email draft by Oct 20" },
    ],
    notes: {
        model_used: null,
        summary:
            "The team reviewed the Q4 roadmap, beta sign-ups and feedback, the new onboarding flow and the launch checklist. Sign-ups are up 24% week over week. Long setup is the top complaint, and the three-step onboarding addresses it by moving invites after the first project.",
        key_topics: ["Q4 roadmap", "Beta sign-ups", "Beta feedback", "New onboarding flow", "Launch checklist", "Billing risk"],
        decisions: [
            { text: "Lock the plan names by Friday so billing can start", made_by: "Priya" },
            { text: "Send team invites after the first project, not during setup", made_by: "Dana" },
            { text: "Test the new onboarding with five beta teams before building it", made_by: "Priya" },
        ],
        action_items: [
            { task: "Run onboarding sessions with five beta teams", assignee: "Dana", due_date: "Next week" },
            { task: "Set up the status page", assignee: "Marcus", due_date: null },
            { task: "Draft the launch email", assignee: "Dana", due_date: "Oct 20" },
            { task: "Add the billing load test to the risk list", assignee: "Marcus", due_date: null },
            { task: "Check in on open risks", assignee: "Priya", due_date: "Tuesday, Oct 13" },
        ],
    },
    links: [
        { url: "https://example.com/roadmap", sources: ["said"], said: 1, first: "5:48", firstSource: "said" },
        { url: "https://example.com/acme/load-test", sources: ["said"], said: 1, first: "17:34", firstSource: "said" },
        {
            url: "https://example.com/acme/launch-plan",
            title: "Q4 launch plan",
            note: "The checklist and owners from this meeting.",
            sources: ["added"],
        },
    ],
};

// ── BIO 101: Cell Biology (Class · BIO 101) ────────────────────────────

const bio: DemoMeeting = {
    id: "m-bio-membrane",
    title: "BIO 101: Cell Biology",
    kind: "class",
    notebook: "BIO 101",
    daysAgo: 1,
    startTime: "09:00",
    durationMin: 50,
    plannedMinutes: 60,
    frames: [
        { at: "0:14", image: "bio-1" },
        { at: "4:10", image: "bio-2" },
        { at: "11:37", image: "bio-3" },
        { at: "15:42", image: "bio-2" },
        { at: "19:07", image: "bio-4" },
        { at: "24:16", image: "bio-4" },
        { at: "27:07", image: "bio-5" },
        { at: "36:12", image: "bio-6" },
        { at: "46:07", image: "bio-7" },
    ],
    lines: [
        { at: "0:12", speaker: "Dr. Reyes", text: "Good morning. Today is all about the cell membrane: what it's made of, and how things get across it." },
        { at: "1:05", speaker: "Dr. Reyes", text: "Every cell has one. It decides what comes in and what stays out." },
        { at: "4:08", speaker: "Dr. Reyes", text: "This is the phospholipid bilayer. Each phospholipid has a head that likes water and two tails that avoid it." },
        { at: "5:20", speaker: "Dr. Reyes", text: "So the heads face the water on both sides, and the tails hide in the middle." },
        { at: "6:45", speaker: "Student", text: "Is that why oil and water don't mix?" },
        { at: "7:10", speaker: "Dr. Reyes", text: "Exactly. Same chemistry. The tails are hydrophobic, like oil." },
        { at: "11:35", speaker: "Dr. Reyes", text: "We call this the fluid mosaic model. The lipids drift sideways, and proteins float in them like tiles." },
        { at: "13:02", speaker: "Dr. Reyes", text: "Cholesterol keeps the membrane from getting too stiff or too runny." },
        { at: "15:40", speaker: "Dr. Reyes", text: "This will be on the exam: name the four parts of the membrane and what each one does." },
        { at: "19:05", speaker: "Dr. Reyes", text: "Now transport. Passive transport needs no energy. Molecules move from high to low concentration." },
        { at: "21:30", speaker: "Dr. Reyes", text: "Oxygen slips right through. Ions need a channel protein. That's facilitated diffusion." },
        { at: "24:12", speaker: "Student", text: "Does facilitated diffusion use ATP?" },
        { at: "24:30", speaker: "Dr. Reyes", text: "No. It still goes downhill, from high to low. The channel just opens a door." },
        { at: "27:05", speaker: "Dr. Reyes", text: "Osmosis is the diffusion of water. Water moves toward the side with more solute." },
        { at: "29:20", speaker: "Dr. Reyes", text: "Put a red blood cell in pure water, a hypotonic solution, and it swells and can burst." },
        { at: "31:00", speaker: "Dr. Reyes", text: "In a hypertonic solution, like salt water, it shrinks." },
        { at: "36:10", speaker: "Dr. Reyes", text: "Active transport moves things against the gradient, and that costs ATP." },
        { at: "38:00", speaker: "Dr. Reyes", text: "The sodium-potassium pump moves three sodium ions out and two potassium ions in for every ATP." },
        { at: "39:30", speaker: "Dr. Reyes", text: "Three out, two in. Write that down." },
        { at: "46:05", speaker: "Dr. Reyes", text: "For Friday, read chapter seven, sections one through three. The syllabus is at example.com/bio101." },
        { at: "47:30", speaker: "Dr. Reyes", text: "Quiz on membranes next Wednesday. See you Friday." },
    ],
    markers: [
        { at: "15:44", kind: "test", note: "Four parts of the membrane" },
        { at: "24:14", kind: "question", note: "Does facilitated diffusion use ATP?" },
        { at: "39:33", kind: "important", note: "3 Na⁺ out, 2 K⁺ in per ATP" },
        { at: "47:33", kind: "test", note: "Quiz next Wednesday" },
    ],
    notes: {
        model_used: "lecture-notes",
        summary:
            "The lecture covered the structure of the cell membrane and how substances cross it. The phospholipid bilayer forms the barrier; proteins, cholesterol and carbohydrate chains make up the fluid mosaic. Passive transport moves substances down their concentration gradient without energy, while active transport, like the sodium-potassium pump, uses ATP to move them against it.",
        key_topics: [
            "Phospholipid bilayer: hydrophilic heads, hydrophobic tails",
            "Fluid mosaic model",
            "Passive transport: diffusion and facilitated diffusion",
            "Osmosis in hypotonic, isotonic and hypertonic solutions",
            "Active transport and the Na⁺/K⁺ pump",
        ],
        decisions: [
            { text: "Hydrophobic: avoids water", context: "the lipid tails, like oil" },
            { text: "Facilitated diffusion: passive transport through a channel or carrier protein", context: "ions crossing through a channel" },
            { text: "Osmosis: diffusion of water toward the side with more solute", context: "a red blood cell swelling in pure water" },
            { text: "Active transport: movement against the gradient that costs ATP", context: "3 Na⁺ out, 2 K⁺ in per ATP" },
        ],
        action_items: [
            { task: "Read chapter 7, sections 1–3", due_date: "Friday" },
            { task: "Quiz on membranes", due_date: "Next Wednesday" },
        ],
    },
    links: [
        { url: "https://example.com/bio101", sources: ["said"], said: 1, first: "46:05", firstSource: "said" },
        {
            url: "https://example.com/bio101/chapter-7",
            title: "Chapter 7 reading",
            note: "Sections 1–3 before Friday.",
            sources: ["added"],
        },
    ],
    study: {
        summary: {
            title: "The Cell Membrane",
            sections: [
                {
                    heading: "Structure",
                    bullets: [
                        "A phospholipid bilayer: hydrophilic heads face the water, hydrophobic tails face inward",
                        "Proteins, cholesterol and carbohydrate chains sit in the bilayer (the fluid mosaic model)",
                        "Cholesterol keeps the membrane fluid across temperatures",
                    ],
                },
                {
                    heading: "Passive transport (no ATP)",
                    bullets: [
                        "Diffusion: from high to low concentration",
                        "Facilitated diffusion: through channel or carrier proteins",
                        "Osmosis: water moves toward the side with more solute",
                    ],
                },
                {
                    heading: "Active transport (uses ATP)",
                    bullets: ["Moves substances against their gradient", "Na⁺/K⁺ pump: 3 Na⁺ out, 2 K⁺ in for each ATP"],
                },
            ],
        },
        terms: {
            terms: [
                { term: "Phospholipid", definition: "A lipid with a water-loving head and two water-avoiding tails; the building block of the membrane." },
                { term: "Hydrophobic", definition: "Avoids water. The tails of phospholipids are hydrophobic." },
                { term: "Fluid mosaic model", definition: "The membrane as a fluid lipid layer with proteins floating in it." },
                { term: "Facilitated diffusion", definition: "Passive movement through a channel or carrier protein." },
                { term: "Osmosis", definition: "Diffusion of water across a membrane toward the side with more solute." },
                { term: "Hypotonic", definition: "A solution with less solute than the cell; water flows in and the cell swells." },
                { term: "Hypertonic", definition: "A solution with more solute than the cell; water flows out and the cell shrinks." },
                { term: "Active transport", definition: "Movement against a concentration gradient that uses ATP." },
            ],
        },
        flashcards: {
            cards: [
                { front: "What does the Na⁺/K⁺ pump move for each ATP?", back: "3 sodium ions out of the cell and 2 potassium ions in." },
                { front: "Which part of a phospholipid is hydrophobic?", back: "The two fatty-acid tails. They face the middle of the bilayer." },
                { front: "Does facilitated diffusion use ATP?", back: "No. It is passive: substances still move from high to low concentration, through a protein." },
                { front: "What happens to a red blood cell in a hypotonic solution?", back: "Water moves in, so the cell swells and can burst." },
                { front: "What does cholesterol do in the membrane?", back: "It keeps the membrane fluid: not too stiff when cold, not too runny when warm." },
                { front: "What is osmosis?", back: "The diffusion of water across a membrane toward the side with more solute." },
            ],
        },
        quiz: {
            questions: [
                {
                    question: "A red blood cell is placed in salt water (a hypertonic solution). What happens?",
                    choices: ["It swells and bursts", "It shrinks as water leaves", "Nothing: there is no net movement", "It pumps salt in with ATP"],
                    answer: 1,
                    explanation: "The solution has more solute than the cell, so water moves out of the cell by osmosis and the cell shrinks.",
                    at_ms: 31 * 60_000,
                },
                {
                    question: "Which kind of transport needs ATP?",
                    choices: ["Simple diffusion", "Facilitated diffusion", "Osmosis", "The sodium-potassium pump"],
                    answer: 3,
                    explanation: "The Na⁺/K⁺ pump is active transport: it moves ions against their gradient, which costs ATP.",
                    at_ms: 36 * 60_000 + 10_000,
                },
                {
                    question: "Why do phospholipid tails face the middle of the bilayer?",
                    choices: ["They are hydrophobic and avoid water", "They carry a charge", "Proteins hold them there", "They bind to cholesterol"],
                    answer: 0,
                    explanation: "The tails are hydrophobic, so they hide from the water on both sides, like oil in water.",
                    at_ms: 5 * 60_000 + 20_000,
                },
            ],
        },
        questions: {
            questions: [
                { question: "How do cells control which ions their channel proteins let through?", at_ms: 21 * 60_000 + 30_000 },
                { question: "What happens to the membrane if there is too little cholesterol?", at_ms: 13 * 60_000 + 2_000 },
            ],
        },
    },
};

// ── Dr. visit notes (Personal · Health) ───────────────────────────────

const health: DemoMeeting = {
    id: "m-health-visit",
    title: "Dr. visit notes",
    kind: "personal",
    notebook: "Health",
    daysAgo: 4,
    startTime: "15:30",
    durationMin: 12,
    plannedMinutes: 15,
    lines: [
        { at: "0:06", speaker: "Dr. Lin", text: "Good to see you. Everything from last time looks good." },
        { at: "0:40", speaker: "Me", text: "Great. I've been sleeping better since we talked." },
        { at: "1:32", speaker: "Dr. Lin", text: "Your blood pressure is right where it should be." },
        { at: "3:10", speaker: "Dr. Lin", text: "Your vitamin D is a little low. Take one supplement a day with breakfast." },
        { at: "4:25", speaker: "Me", text: "With breakfast. Got it." },
        { at: "6:02", speaker: "Dr. Lin", text: "Let's do routine bloodwork before your next visit." },
        { at: "8:15", speaker: "Dr. Lin", text: "Keep up the walking. Thirty minutes most days is perfect." },
        { at: "10:40", speaker: "Dr. Lin", text: "Book a follow-up for three months from now." },
    ],
    markers: [{ at: "3:14", kind: "test", note: "Vitamin D with breakfast" }],
    notes: {
        model_used: "personal-notes",
        summary: "A routine checkup. Blood pressure is normal and sleep has improved. Vitamin D is slightly low, so a daily supplement was suggested.",
        key_topics: ["Blood pressure is normal", "Vitamin D slightly low", "Keep walking 30 minutes most days"],
        decisions: [],
        action_items: [
            { task: "Take vitamin D daily with breakfast", due_date: "Starting tomorrow" },
            { task: "Schedule routine bloodwork", due_date: "Before the next visit" },
            { task: "Book a follow-up visit", due_date: "In 3 months" },
        ],
    },
};

// ── Older recordings (library filler) ─────────────────────────────────
// Kept short: with more rows than fit, the compact list beside an open
// recording squeezes its Notebooks chip row (a flex quirk in the app).

const older: DemoMeeting[] = [
    { id: "m-old-design", title: "Design review: settings page", kind: "meeting", notebook: "Acme project", daysAgo: 5, startTime: "14:00", durationMin: 34, plannedMinutes: 60 },
    { id: "m-old-cells", title: "BIO 101: Intro to cells", kind: "class", notebook: "BIO 101", daysAgo: 6, startTime: "09:00", durationMin: 50, plannedMinutes: 60 },
    { id: "m-old-lab", title: "BIO 101: Lab safety", kind: "class", notebook: "BIO 101", daysAgo: 11, startTime: "09:00", durationMin: 45, plannedMinutes: 60 },
    { id: "m-old-chem", title: "BIO 101: Chemistry of life", kind: "class", notebook: "BIO 101", daysAgo: 13, startTime: "09:00", durationMin: 50, plannedMinutes: 60 },
];

export const DEMO_MEETINGS: DemoMeeting[] = [acme, bio, health, ...older];

/** Most recent first, as list_recent_notebooks returns them */
export const RECENT_NOTEBOOKS = ["Acme project", "BIO 101", "Health"];

// ── Live recording scripts (what streams in while "recording") ──────────

export interface LiveScript {
    title: string;
    lines: { speaker: string; text: string }[];
    /** Live insight cards: seconds after the recording really started
     *  (not counting a backdated prefill), type, text */
    insights: { at: number; type: string; text: string; assignee?: string }[];
    /** Screens captured while recording (frames/<id>-thumb.jpg) */
    frames: string[];
}

export const LIVE_SCRIPTS: Record<Kind, LiveScript> = {
    meeting: {
        title: "Acme launch check-in",
        lines: [
            { speaker: "Priya", text: "Okay, quick check-in on the launch. Let's start with the checklist." },
            { speaker: "Marcus", text: "The status page is live in staging. I'll switch it on Thursday." },
            { speaker: "Dana", text: "The onboarding sessions went well. Four of five teams finished setup in under two minutes." },
            { speaker: "Priya", text: "That's a big jump. Last time it took them almost ten." },
            { speaker: "Dana", text: "Moving the invites after the first project made the biggest difference." },
            { speaker: "Marcus", text: "The billing load test is booked for Monday morning." },
            { speaker: "Priya", text: "Great. Then the only open item is the launch email." },
            { speaker: "Dana", text: "I'll send the draft around tonight." },
            { speaker: "Priya", text: "Perfect. Let's lock the launch for December eighth." },
            { speaker: "Marcus", text: "Works for me." },
        ],
        insights: [
            { at: 0, type: "decision", text: "Status page goes live on Thursday." },
            { at: 3, type: "action_item", text: "Send the launch email draft around tonight.", assignee: "Dana" },
            { at: 8, type: "decision", text: "Launch date locked: December 8." },
        ],
        frames: ["acme-6", "acme-5", "acme-3", "acme-2"],
    },
    // The lecture after "BIO 101: Cell Biology": same slides, so the
    // captured screens match. The 5th line ("…on the exam") streams in
    // just before the Mark clip clicks Mark (4 lines prefilled).
    class: {
        title: "BIO 101: Membrane transport",
        lines: [
            { speaker: "Dr. Reyes", text: "Okay, let's pick up where we left off: how things get across the membrane." },
            { speaker: "Dr. Reyes", text: "Small molecules like oxygen slip right through the bilayer. That's simple diffusion." },
            { speaker: "Dr. Reyes", text: "Ions can't. They need a channel protein, and that's facilitated diffusion." },
            { speaker: "Dr. Reyes", text: "Active transport is different: the sodium-potassium pump moves three sodium out and two potassium in for every ATP." },
            { speaker: "Dr. Reyes", text: "Write that one down. It will be on the exam." },
            { speaker: "Student", text: "Is that on Wednesday's quiz too?" },
            { speaker: "Dr. Reyes", text: "Yes. Membranes and transport, all of it." },
            { speaker: "Dr. Reyes", text: "Now, why does the cell spend energy on this? Let's look at what the gradient does." },
        ],
        insights: [
            { at: 0, type: "topic_shift", text: "Membrane transport: passive vs. active" },
            { at: 6, type: "key_point", text: "Diffusion and facilitated diffusion need no ATP." },
            { at: 14, type: "key_point", text: "Na⁺/K⁺ pump: 3 out, 2 in per ATP. On the exam." },
        ],
        frames: ["bio-1", "bio-2", "bio-4", "bio-6"],
    },
    personal: {
        title: "Personal note",
        lines: [
            { speaker: "Me", text: "Quick note before I forget: book the follow-up for January." },
            { speaker: "Me", text: "And pick up the vitamin D on the way home." },
        ],
        insights: [],
        frames: [],
    },
};
