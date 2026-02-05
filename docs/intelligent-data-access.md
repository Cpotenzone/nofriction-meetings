# Intelligent Data Access System - Complete Documentation

## Overview

The Intelligent Data Access System enables a RAG (Retrieval Augmented Generation) pipeline for the noFriction Meetings chatbot. When you ask a question, the system searches your historical data (meetings, transcripts, past conversations) for relevant context, then sends that context along with your question to TheBrain AI for an intelligent response.

---

## How It Works: End-to-End Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            USER ASKS A QUESTION                             │
│                   "What did we discuss about the budget?"                   │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                          1. FRONTEND (React/TypeScript)                     │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │ AIChat.tsx / CopilotPanel.tsx                                        │   │
│  │ • Captures user input                                                │   │
│  │ • Checks if RAG is enabled (toggle switch)                          │   │
│  │ • Calls Tauri command: thebrain_rag_chat_with_memory                │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         2. BACKEND (Rust/Tauri)                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │ commands.rs :: thebrain_rag_chat_with_memory()                       │   │
│  │                                                                       │   │
│  │ Step A: VECTOR SEARCH                                                │   │
│  │ ├─ Calls Pinecone with user's question                              │   │
│  │ ├─ Pinecone converts text → embedding (llama-text-embed-v2)         │   │
│  │ └─ Returns top 5 similar documents with scores                      │   │
│  │                                                                       │   │
│  │ Step B: BUILD AUGMENTED PROMPT                                       │   │
│  │ ├─ Filters results by score > 0.5 (relevance threshold)            │   │
│  │ └─ Constructs: "Context: [history] | Question: [user input]"        │   │
│  │                                                                       │   │
│  │ Step C: CALL THEBRAIN                                                │   │
│  │ ├─ POST /api/chat/stream with augmented prompt                      │   │
│  │ └─ Returns AI response                                               │   │
│  │                                                                       │   │
│  │ Step D: STORE CONVERSATION (for future retrieval)                   │   │
│  │ ├─ Upsert to Pinecone (vectorized Q&A)                              │   │
│  │ └─ Insert to Supabase (structured record)                           │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                       3. RESPONSE DISPLAYED TO USER                         │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │ Frontend shows:                                                       │   │
│  │ • AI response text                                                    │   │
│  │ • Expandable "📚 X sources used" with context cards                  │   │
│  │ • Each card shows: match score, timestamp, summary snippet           │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Component Breakdown

### 1. Frontend Components

| Component | Location | Purpose |
|-----------|----------|---------|
| `AIChat.tsx` | `src/components/` | Main AI chat interface with RAG toggle |
| `CopilotPanel.tsx` | `src/components/` | Compact side panel version |

**Key Features:**
- **RAG Toggle**: Checkbox to enable/disable history search
- **Context Cards**: Shows sources used in generating the response
- **Quick Actions**: "Summary", "Tasks", "History" buttons

### 2. Backend Commands (Rust)

| Command | Purpose |
|---------|---------|
| `thebrain_rag_chat` | RAG search + TheBrain call (no storage) |
| `thebrain_rag_chat_with_memory` | Same + auto-stores conversation |
| `store_conversation` | Manual conversation storage |
| `get_conversation_history` | Retrieve past conversations |

### 3. Vector Database (Pinecone)

**Index Configuration:**
- Namespace: Configurable (e.g., `nofriction-prod`)
- Embedding Model: `llama-text-embed-v2` (integrated)
- Dimensions: Automatic based on model

**What Gets Stored:**
- Meeting transcripts
- VLM image analysis results  
- Activity summaries
- Chat conversations (Q&A pairs)

### 4. SQL Database (Supabase)

**Table: `conversations`**
```sql
id              UUID        -- Unique conversation ID
timestamp       TIMESTAMPTZ -- When it occurred
user_query      TEXT        -- What the user asked
assistant_response TEXT     -- AI's answer
model_used      VARCHAR     -- Which model (e.g., qwen3:8b)
context_refs    JSONB       -- Array of Pinecone IDs used
```

### 5. Server Intelligence (nofriction-intel)

**vlm.py** - Vision Language Model client
- Analyzes screenshots/frames
- Extracts entities, context, summaries
- Uses TheBrain OAuth authentication

**llm.py** - Language Model client  
- Synthesizes moments (combines VLM + transcripts)
- Text-only chat functions
- Same OAuth pattern as VLM

---

## Authentication Flow

```
┌─────────────┐    POST /api/token     ┌─────────────┐
│   Client    │ ───────────────────▶  │  TheBrain   │
│             │    {username,password} │   Cloud     │
│             │ ◀───────────────────  │             │
│             │    {access_token}      │             │
└─────────────┘                        └─────────────┘
       │
       │ Token cached locally
       │
       ▼
┌─────────────────────────────────────────────────────┐
│ All subsequent requests include:                     │
│ Authorization: Bearer <access_token>                 │
│                                                      │
│ If 401 returned → refresh token → retry             │
└─────────────────────────────────────────────────────┘
```

---

## Data Flow Example

**User asks:** "What budget numbers did John mention?"

1. **Search Phase:**
   - Pinecone returns 3 relevant chunks:
     - Meeting transcript from Jan 15 (score: 0.87)
     - Previous Q&A about finances (score: 0.72)
     - Budget spreadsheet analysis (score: 0.65)

2. **Prompt Construction:**
   ```
   You are an AI assistant with access to the user's meeting history.
   
   CONTEXT:
   [1] (87% match, 2026-01-15): "John mentioned the Q1 budget is $2.4M..."
   [2] (72% match, 2026-01-10): "User asked about budget allocation..."
   [3] (65% match, 2026-01-08): "Spreadsheet shows marketing: $500K..."
   
   USER QUESTION: What budget numbers did John mention?
   ```

3. **TheBrain Response:**
   "Based on the meeting from January 15th, John mentioned that the Q1 budget is $2.4 million. The breakdown shows marketing allocated at $500K according to the spreadsheet analysis from January 8th."

4. **Storage:**
   - This Q&A pair is vectorized and stored
   - Future questions about budgets will find this conversation

---

## Environment Configuration

### Desktop App (Tauri)
Configured via Settings UI:
- TheBrain credentials (username/password)
- Pinecone API key, index host, namespace
- Supabase connection string

### Server (nofriction-intel)
```bash
# .env file
VLM_BASE_URL=https://7wk68vrq9achr2djw.caas.targon.com
VLM_USERNAME=your_username
VLM_PASSWORD=your_password
VLM_MODEL_PRIMARY=qwen3-vl:8b
VLM_MODEL_FALLBACK=qwen2.5vl:7b
```

---

## Key Files Reference

| File | Purpose |
|------|---------|
| `src-tauri/src/commands.rs` | RAG chat commands |
| `src-tauri/src/pinecone_client.rs` | Vector search/upsert |
| `src-tauri/src/vlm_client.rs` | TheBrain API client |
| `src/components/AIChat.tsx` | Main chat UI |
| `src/components/CopilotPanel.tsx` | Side panel chat |
| `nofriction-intel/app/vlm.py` | Server VLM client |
| `nofriction-intel/app/llm.py` | Server LLM client |
| `supabase/migrations/` | Database schema |

---

## Future Enhancements (Phase 5)

- **Conversation Threading**: Group related Q&As
- **Query Suggestions**: Auto-suggest based on history
- **Daily Briefings**: Generate morning summaries
