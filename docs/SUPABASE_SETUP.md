# Supabase Database Setup

This guide covers deploying the Supabase database schema for noFriction Meetings.

---

## Quick Start

### Option 1: Supabase Dashboard (Recommended)

1. Go to your [Supabase Dashboard](https://supabase.com/dashboard)
2. Select your project
3. Navigate to **SQL Editor**
4. Copy and paste the migration files below
5. Click **Run**

### Option 2: Supabase CLI

```bash
# Install CLI if not already
brew install supabase/tap/supabase

# Link to your project
supabase link --project-ref YOUR_PROJECT_REF

# Push migrations
supabase db push
```

---

## Required Migrations

### 1. Conversations Table

**File:** `supabase/migrations/20260203_create_conversations_table.sql`

```sql
-- Supabase Migration: Create conversations table for RAG chat history
-- Run this in your Supabase SQL editor

-- Conversations table for storing chat Q&A pairs
CREATE TABLE IF NOT EXISTS conversations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    user_query TEXT NOT NULL,
    assistant_response TEXT NOT NULL,
    model_used VARCHAR(100) NOT NULL,
    context_refs JSONB DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Index for timestamp-based queries
CREATE INDEX IF NOT EXISTS idx_conversations_timestamp ON conversations(timestamp DESC);

-- Index for full-text search on queries and responses
CREATE INDEX IF NOT EXISTS idx_conversations_query_text ON conversations USING GIN(to_tsvector('english', user_query));
CREATE INDEX IF NOT EXISTS idx_conversations_response_text ON conversations USING GIN(to_tsvector('english', assistant_response));

-- Trigger for updated_at
CREATE OR REPLACE FUNCTION update_conversations_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER conversations_updated_at_trigger
    BEFORE UPDATE ON conversations
    FOR EACH ROW
    EXECUTE FUNCTION update_conversations_updated_at();

-- Grant permissions
GRANT ALL ON conversations TO authenticated;
GRANT ALL ON conversations TO service_role;

-- Comment for documentation
COMMENT ON TABLE conversations IS 'Stores RAG chatbot conversation history for retrieval and learning';
COMMENT ON COLUMN conversations.context_refs IS 'JSON array of Pinecone vector IDs used as context for this response';
```

### 2. Activities Table (if not exists)

```sql
-- Activities table for VLM-analyzed content
CREATE TABLE IF NOT EXISTS activities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    category VARCHAR(100) NOT NULL,
    summary TEXT NOT NULL,
    entities JSONB DEFAULT '[]'::jsonb,
    app_name VARCHAR(255),
    focus_area VARCHAR(255),
    source_type VARCHAR(50) DEFAULT 'screen_capture',
    meeting_id UUID,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_activities_timestamp ON activities(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_activities_category ON activities(category);
CREATE INDEX IF NOT EXISTS idx_activities_meeting ON activities(meeting_id);
CREATE INDEX IF NOT EXISTS idx_activities_text ON activities USING GIN(to_tsvector('english', summary));

-- Grant permissions
GRANT ALL ON activities TO authenticated;
GRANT ALL ON activities TO service_role;
```

### 3. Meetings Table (if not exists)

```sql
-- Meetings table
CREATE TABLE IF NOT EXISTS meetings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title VARCHAR(500),
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ,
    duration_seconds INTEGER,
    transcript_count INTEGER DEFAULT 0,
    frame_count INTEGER DEFAULT 0,
    participants JSONB DEFAULT '[]'::jsonb,
    summary TEXT,
    action_items JSONB DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_meetings_start_time ON meetings(start_time DESC);
CREATE INDEX IF NOT EXISTS idx_meetings_title ON meetings USING GIN(to_tsvector('english', title));

-- Grant permissions
GRANT ALL ON meetings TO authenticated;
GRANT ALL ON meetings TO service_role;
```

---

## Verification

After running migrations, verify the tables exist:

```sql
-- Check tables
SELECT table_name FROM information_schema.tables 
WHERE table_schema = 'public' 
ORDER BY table_name;

-- Should show:
-- activities
-- conversations
-- meetings
```

---

## Connection Configuration

### In noFriction Meetings App

1. Open **Settings** → **Knowledge Base**
2. Enter your Supabase credentials:
   - **Supabase URL**: `https://YOUR_PROJECT.supabase.co`
   - **Supabase Key**: Your `service_role` or `anon` key

### Connection String Format

```
postgres://postgres:[PASSWORD]@db.[PROJECT_REF].supabase.co:5432/postgres
```

---

## Row Level Security (Optional)

If using Supabase Auth for multi-user:

```sql
-- Enable RLS
ALTER TABLE conversations ENABLE ROW LEVEL SECURITY;
ALTER TABLE activities ENABLE ROW LEVEL SECURITY;
ALTER TABLE meetings ENABLE ROW LEVEL SECURITY;

-- Create policy (users can only see their own data)
CREATE POLICY "Users can view own conversations"
ON conversations FOR SELECT
USING (auth.uid() = user_id);

-- Note: You'd need to add a user_id column first
ALTER TABLE conversations ADD COLUMN user_id UUID REFERENCES auth.users(id);
```

---

## Troubleshooting

### "Permission denied" Error

Ensure you're using the `service_role` key, not the `anon` key:
```
Settings → API → service_role key (secret)
```

### "Table already exists" Error

Safe to ignore - migrations use `IF NOT EXISTS`.

### Connection Refused

Check your project is active:
- Free tier pauses after 1 week of inactivity
- Resume from Supabase Dashboard

---

## Backup & Restore

### Export Data

```bash
# Using pg_dump
pg_dump "postgres://..." > backup.sql
```

### Restore Data

```bash
psql "postgres://..." < backup.sql
```

---

## Schema Diagram

```
┌─────────────────────┐      ┌─────────────────────┐
│     conversations   │      │      activities     │
├─────────────────────┤      ├─────────────────────┤
│ id (PK)             │      │ id (PK)             │
│ timestamp           │      │ timestamp           │
│ user_query          │      │ category            │
│ assistant_response  │      │ summary             │
│ model_used          │      │ entities (JSONB)    │
│ context_refs (JSON) │      │ app_name            │
│ created_at          │      │ focus_area          │
│ updated_at          │      │ meeting_id (FK)─────┼──┐
└─────────────────────┘      │ created_at          │  │
                             └─────────────────────┘  │
                                                      │
                             ┌─────────────────────┐  │
                             │      meetings       │  │
                             ├─────────────────────┤  │
                             │ id (PK)◄────────────┼──┘
                             │ title               │
                             │ start_time          │
                             │ end_time            │
                             │ duration_seconds    │
                             │ transcript_count    │
                             │ frame_count         │
                             │ participants (JSON) │
                             │ summary             │
                             │ action_items (JSON) │
                             │ created_at          │
                             │ updated_at          │
                             └─────────────────────┘
```
