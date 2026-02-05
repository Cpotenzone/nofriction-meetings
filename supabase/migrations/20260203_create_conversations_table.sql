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

-- Row Level Security (optional - enable if using Supabase Auth)
-- ALTER TABLE conversations ENABLE ROW LEVEL SECURITY;

-- Grant permissions
GRANT ALL ON conversations TO authenticated;
GRANT ALL ON conversations TO service_role;

-- Comment for documentation
COMMENT ON TABLE conversations IS 'Stores RAG chatbot conversation history for retrieval and learning';
COMMENT ON COLUMN conversations.context_refs IS 'JSON array of Pinecone vector IDs used as context for this response';
