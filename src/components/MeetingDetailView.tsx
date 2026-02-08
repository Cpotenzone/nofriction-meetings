// Meeting Detail View
// Comprehensive meeting analysis interface with tabs for Transcript, Notes, Study, Comments

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RewindTab } from './RewindTab';

// Types
interface Meeting {
  id: string;
  title: string;
  started_at: string;
  ended_at: string | null;
  duration_seconds: number | null;
}

interface Transcript {
  id: number;
  meeting_id: string;
  text: string;
  speaker: string | null;
  timestamp: string;
  is_final: boolean;
  confidence: number;
}

interface MeetingNotes {
  id: string;
  meeting_id: string;
  summary: string | null;
  key_topics: string | null;
  decisions: string | null;
  action_items: string | null;
  participants: string | null;
  generated_at: string;
  model_used: string | null;
}

interface MeetingComment {
  id: string;
  meeting_id: string;
  user_id: string | null;
  comment: string;
  comment_type: string;
  timestamp_ref: number | null;
  created_at: string;
  updated_at: string | null;
  parent_id: string | null;
}

interface MeetingAnalysis {
  meeting: Meeting;
  transcripts: Transcript[];
  notes: MeetingNotes | null;
  comments: MeetingComment[];
  transcript_count: number;
  comment_count: number;
  has_notes: boolean;
}

interface GeneratedNotes {
  summary: string;
  key_topics: string[];
  decisions: { text: string; made_by: string | null; context: string | null }[];
  action_items: { task: string; assignee: string | null; priority: string | null }[];
  participants: string[];
}

type TabType = 'transcript' | 'notes' | 'rewind' | 'study' | 'comments';

interface MeetingDetailViewProps {
  meetingId: string;
  onClose?: () => void;
}

export function MeetingDetailView({ meetingId, onClose }: MeetingDetailViewProps) {
  const [activeTab, setActiveTab] = useState<TabType>('transcript');
  const [analysis, setAnalysis] = useState<MeetingAnalysis | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [generatingNotes, setGeneratingNotes] = useState(false);
  const [newComment, setNewComment] = useState('');
  const [commentType, setCommentType] = useState<'note' | 'decision' | 'action' | 'question'>('note');

  // Fetch meeting analysis
  const fetchAnalysis = useCallback(async () => {
    try {
      setLoading(true);
      const result = await invoke<MeetingAnalysis>('get_meeting_analysis', { meeting_id: meetingId });
      setAnalysis(result);
      setError(null);
    } catch (e) {
      setError(e as string);
    } finally {
      setLoading(false);
    }
  }, [meetingId]);

  useEffect(() => {
    fetchAnalysis();
  }, [fetchAnalysis]);

  // Generate AI notes
  const handleGenerateNotes = async () => {
    try {
      setGeneratingNotes(true);
      await invoke<GeneratedNotes>('generate_meeting_notes', { meeting_id: meetingId });
      await fetchAnalysis();
    } catch (e) {
      setError(`Failed to generate notes: ${e}`);
    } finally {
      setGeneratingNotes(false);
    }
  };

  // Add comment
  const handleAddComment = async () => {
    if (!newComment.trim()) return;
    try {
      await invoke('add_meeting_comment', {
        meeting_id: meetingId,
        comment: newComment,
        comment_type: commentType,
        timestamp_ref: null,
        parent_id: null,
      });
      setNewComment('');
      await fetchAnalysis();
    } catch (e) {
      setError(`Failed to add comment: ${e}`);
    }
  };

  if (loading) {
    return (
      <div className="meeting-detail-view loading">
        <div className="spinner" />
        <span>Loading meeting...</span>
      </div>
    );
  }

  if (error || !analysis) {
    return (
      <div className="meeting-detail-view error">
        <span>❌ {error || 'Meeting not found'}</span>
        {onClose && <button onClick={onClose}>Close</button>}
      </div>
    );
  }

  const { meeting, transcripts, notes, comments } = analysis;

  return (
    <div className="meeting-detail-view">
      {/* Header */}
      <header className="meeting-header">
        <div className="meeting-info">
          <h2>{meeting.title}</h2>
          <span className="meeting-meta">
            {new Date(meeting.started_at).toLocaleDateString()} •
            {meeting.duration_seconds ? ` ${Math.round(meeting.duration_seconds / 60)} min` : ' In progress'}
          </span>
        </div>
        {onClose && (
          <button className="close-btn" onClick={onClose}>✕</button>
        )}
      </header>

      {/* Tabs */}
      <nav className="meeting-tabs">
        {(['transcript', 'notes', 'rewind', 'study', 'comments'] as TabType[]).map((tab) => (
          <button
            key={tab}
            className={`tab ${activeTab === tab ? 'active' : ''}`}
            onClick={() => setActiveTab(tab)}
          >
            {tab === 'transcript' && `📝 Transcript (${transcripts.length})`}
            {tab === 'notes' && `🧠 Notes ${notes ? '✓' : ''}`}
            {tab === 'rewind' && '⏪ Rewind'}
            {tab === 'study' && '📚 Study'}
            {tab === 'comments' && `💬 Comments (${comments.length})`}
          </button>
        ))}
      </nav>

      {/* Tab Content */}
      <div className="tab-content">
        {activeTab === 'transcript' && (
          <TranscriptTab transcripts={transcripts} />
        )}

        {activeTab === 'notes' && (
          <NotesTab
            notes={notes}
            onGenerate={handleGenerateNotes}
            generating={generatingNotes}
          />
        )}

        {activeTab === 'rewind' && (
          <RewindTab meetingId={meetingId} />
        )}

        {activeTab === 'study' && (
          <StudyTab meetingId={meetingId} />
        )}

        {activeTab === 'comments' && (
          <CommentsTab
            comments={comments}
            newComment={newComment}
            setNewComment={setNewComment}
            commentType={commentType}
            setCommentType={setCommentType}
            onAdd={handleAddComment}
          />
        )}
      </div>

      <style>{`
        .meeting-detail-view {
          display: flex;
          flex-direction: column;
          height: 100%;
          background: var(--bg-primary, #1a1a2e);
          color: var(--text-primary, #e0e0e0);
          border-radius: 12px;
          overflow: hidden;
        }

        .meeting-detail-view.loading,
        .meeting-detail-view.error {
          display: flex;
          align-items: center;
          justify-content: center;
          gap: 12px;
          padding: 40px;
        }

        .meeting-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 16px 20px;
          background: linear-gradient(135deg, #2a2a4a 0%, #1a1a2e 100%);
          border-bottom: 1px solid rgba(255,255,255,0.1);
        }

        .meeting-header h2 {
          margin: 0;
          font-size: 1.25rem;
          font-weight: 600;
        }

        .meeting-meta {
          font-size: 0.85rem;
          color: var(--text-secondary, #888);
        }

        .close-btn {
          background: none;
          border: none;
          color: var(--text-secondary, #888);
          font-size: 1.25rem;
          cursor: pointer;
          padding: 4px 8px;
        }

        .close-btn:hover {
          color: var(--text-primary, #e0e0e0);
        }

        .meeting-tabs {
          display: flex;
          gap: 4px;
          padding: 8px 16px;
          background: rgba(0,0,0,0.2);
          border-bottom: 1px solid rgba(255,255,255,0.05);
        }

        .meeting-tabs .tab {
          padding: 8px 16px;
          background: transparent;
          border: none;
          color: var(--text-secondary, #888);
          cursor: pointer;
          border-radius: 6px;
          transition: all 0.2s;
          font-size: 0.9rem;
        }

        .meeting-tabs .tab:hover {
          background: rgba(255,255,255,0.05);
        }

        .meeting-tabs .tab.active {
          background: rgba(59, 130, 246, 0.2);
          color: #3b82f6;
        }

        .tab-content {
          flex: 1;
          overflow-y: auto;
          padding: 16px 20px;
        }

        .spinner {
          width: 24px;
          height: 24px;
          border: 2px solid rgba(255,255,255,0.2);
          border-top-color: #3b82f6;
          border-radius: 50%;
          animation: spin 1s linear infinite;
        }

        @keyframes spin {
          to { transform: rotate(360deg); }
        }
      `}</style>
    </div>
  );
}

// ============================================
// Sub-components
// ============================================

function TranscriptTab({ transcripts }: { transcripts: Transcript[] }) {
  if (transcripts.length === 0) {
    return (
      <div className="empty-state">
        <span>📝</span>
        <p>No transcript available yet.</p>
      </div>
    );
  }

  return (
    <div className="transcript-list">
      {transcripts.map((t) => (
        <div key={t.id} className="transcript-item">
          <div className="transcript-meta">
            <span className="speaker">{t.speaker || 'Speaker'}</span>
            <span className="timestamp">
              {new Date(t.timestamp).toLocaleTimeString()}
            </span>
          </div>
          <p className="transcript-text">{t.text}</p>
        </div>
      ))}
      <style>{`
        .transcript-list {
          display: flex;
          flex-direction: column;
          gap: 12px;
        }

        .transcript-item {
          padding: 12px 16px;
          background: rgba(255,255,255,0.03);
          border-radius: 8px;
          border-left: 3px solid #3b82f6;
        }

        .transcript-meta {
          display: flex;
          justify-content: space-between;
          margin-bottom: 6px;
          font-size: 0.8rem;
        }

        .speaker {
          color: #3b82f6;
          font-weight: 500;
        }

        .timestamp {
          color: var(--text-secondary, #666);
        }

        .transcript-text {
          margin: 0;
          line-height: 1.5;
        }

        .empty-state {
          text-align: center;
          padding: 40px;
          color: var(--text-secondary, #666);
        }

        .empty-state span {
          font-size: 2rem;
        }
      `}</style>
    </div>
  );
}

function NotesTab({
  notes,
  onGenerate,
  generating,
}: {
  notes: MeetingNotes | null;
  onGenerate: () => void;
  generating: boolean;
}) {
  if (!notes) {
    return (
      <div className="notes-empty">
        <span>🧠</span>
        <p>No AI notes generated yet.</p>
        <button
          className="generate-btn"
          onClick={onGenerate}
          disabled={generating}
        >
          {generating ? 'Generating...' : '✨ Generate AI Notes'}
        </button>
        <style>{`
          .notes-empty {
            text-align: center;
            padding: 40px;
          }

          .notes-empty span {
            font-size: 2rem;
          }

          .generate-btn {
            margin-top: 16px;
            padding: 12px 24px;
            background: linear-gradient(135deg, #3b82f6, #8b5cf6);
            color: white;
            border: none;
            border-radius: 8px;
            cursor: pointer;
            font-weight: 500;
            transition: transform 0.2s;
          }

          .generate-btn:hover:not(:disabled) {
            transform: scale(1.02);
          }

          .generate-btn:disabled {
            opacity: 0.6;
            cursor: wait;
          }
        `}</style>
      </div>
    );
  }

  const keyTopics = notes.key_topics ? JSON.parse(notes.key_topics) : [];
  const decisions = notes.decisions ? JSON.parse(notes.decisions) : [];
  const actionItems = notes.action_items ? JSON.parse(notes.action_items) : [];

  return (
    <div className="notes-content">
      {notes.summary && (
        <section className="notes-section">
          <h3>📋 Summary</h3>
          <p>{notes.summary}</p>
        </section>
      )}

      {keyTopics.length > 0 && (
        <section className="notes-section">
          <h3>🎯 Key Topics</h3>
          <ul className="topics-list">
            {keyTopics.map((topic: string, i: number) => (
              <li key={i}>{topic}</li>
            ))}
          </ul>
        </section>
      )}

      {decisions.length > 0 && (
        <section className="notes-section">
          <h3>✅ Decisions</h3>
          <ul className="decisions-list">
            {decisions.map((d: { text: string; made_by?: string }, i: number) => (
              <li key={i}>
                {d.text}
                {d.made_by && <span className="by"> — {d.made_by}</span>}
              </li>
            ))}
          </ul>
        </section>
      )}

      {actionItems.length > 0 && (
        <section className="notes-section">
          <h3>📌 Action Items</h3>
          <ul className="actions-list">
            {actionItems.map((a: { task: string; assignee?: string; priority?: string }, i: number) => (
              <li key={i} className={`priority-${a.priority || 'medium'}`}>
                <span className="task">{a.task}</span>
                {a.assignee && <span className="assignee">→ {a.assignee}</span>}
              </li>
            ))}
          </ul>
        </section>
      )}

      <button className="regenerate-btn" onClick={onGenerate} disabled={generating}>
        {generating ? 'Regenerating...' : '🔄 Regenerate Notes'}
      </button>

      <style>{`
        .notes-content {
          display: flex;
          flex-direction: column;
          gap: 20px;
        }

        .notes-section h3 {
          margin: 0 0 8px 0;
          font-size: 1rem;
          font-weight: 600;
        }

        .notes-section p {
          margin: 0;
          line-height: 1.6;
          color: var(--text-secondary, #ccc);
        }

        .topics-list, .decisions-list, .actions-list {
          margin: 0;
          padding-left: 20px;
        }

        .topics-list li, .decisions-list li, .actions-list li {
          margin-bottom: 6px;
        }

        .by, .assignee {
          color: #3b82f6;
          font-size: 0.9em;
        }

        .priority-high {
          border-left: 3px solid #ef4444;
          padding-left: 8px;
        }

        .priority-medium {
          border-left: 3px solid #f59e0b;
          padding-left: 8px;
        }

        .priority-low {
          border-left: 3px solid #10b981;
          padding-left: 8px;
        }

        .regenerate-btn {
          align-self: flex-start;
          padding: 8px 16px;
          background: rgba(255,255,255,0.1);
          color: var(--text-primary);
          border: none;
          border-radius: 6px;
          cursor: pointer;
        }

        .regenerate-btn:hover:not(:disabled) {
          background: rgba(255,255,255,0.15);
        }
      `}</style>
    </div>
  );
}

function StudyTab({ meetingId }: { meetingId: string }) {
  const [loading, setLoading] = useState(false);

  const handleGenerateStudy = async () => {
    try {
      setLoading(true);
      await invoke('generate_dork_mode_materials', { meeting_id: meetingId });
    } catch (e) {
      console.error('Failed to generate study materials:', e);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="study-content">
      <span className="study-icon">📚</span>
      <h3>Dork Mode Study Materials</h3>
      <p>Generate flashcards, quizzes, and key concepts from this meeting.</p>
      <button
        className="study-btn"
        onClick={handleGenerateStudy}
        disabled={loading}
      >
        {loading ? 'Generating...' : '🎓 Generate Study Materials'}
      </button>
      <style>{`
        .study-content {
          text-align: center;
          padding: 40px;
        }

        .study-icon {
          font-size: 3rem;
        }

        .study-content h3 {
          margin: 16px 0 8px;
        }

        .study-content p {
          color: var(--text-secondary, #888);
          margin-bottom: 20px;
        }

        .study-btn {
          padding: 12px 24px;
          background: linear-gradient(135deg, #10b981, #3b82f6);
          color: white;
          border: none;
          border-radius: 8px;
          cursor: pointer;
          font-weight: 500;
        }

        .study-btn:disabled {
          opacity: 0.6;
        }
      `}</style>
    </div>
  );
}

function CommentsTab({
  comments,
  newComment,
  setNewComment,
  commentType,
  setCommentType,
  onAdd,
}: {
  comments: MeetingComment[];
  newComment: string;
  setNewComment: (v: string) => void;
  commentType: 'note' | 'decision' | 'action' | 'question';
  setCommentType: (v: 'note' | 'decision' | 'action' | 'question') => void;
  onAdd: () => void;
}) {
  const typeIcons: Record<string, string> = {
    note: '📝',
    decision: '✅',
    action: '📌',
    question: '❓',
  };

  return (
    <div className="comments-content">
      {/* Add comment form */}
      <div className="add-comment">
        <div className="comment-type-selector">
          {(['note', 'decision', 'action', 'question'] as const).map((type) => (
            <button
              key={type}
              className={`type-btn ${commentType === type ? 'active' : ''}`}
              onClick={() => setCommentType(type)}
            >
              {typeIcons[type]} {type.charAt(0).toUpperCase() + type.slice(1)}
            </button>
          ))}
        </div>
        <div className="comment-input-row">
          <input
            type="text"
            placeholder={`Add a ${commentType}...`}
            value={newComment}
            onChange={(e) => setNewComment(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && onAdd()}
          />
          <button onClick={onAdd} disabled={!newComment.trim()}>
            Add
          </button>
        </div>
      </div>

      {/* Comments list */}
      <div className="comments-list">
        {comments.length === 0 ? (
          <p className="empty">No comments yet. Add the first one!</p>
        ) : (
          comments.map((c) => (
            <div key={c.id} className={`comment-item type-${c.comment_type}`}>
              <span className="comment-icon">{typeIcons[c.comment_type] || '💬'}</span>
              <div className="comment-body">
                <p>{c.comment}</p>
                <span className="comment-time">
                  {new Date(c.created_at).toLocaleString()}
                </span>
              </div>
            </div>
          ))
        )}
      </div>

      <style>{`
        .comments-content {
          display: flex;
          flex-direction: column;
          gap: 16px;
        }

        .add-comment {
          background: rgba(255,255,255,0.05);
          border-radius: 8px;
          padding: 12px;
        }

        .comment-type-selector {
          display: flex;
          gap: 6px;
          margin-bottom: 10px;
        }

        .type-btn {
          padding: 6px 12px;
          background: rgba(255,255,255,0.1);
          border: none;
          border-radius: 6px;
          color: var(--text-secondary);
          cursor: pointer;
          font-size: 0.85rem;
        }

        .type-btn.active {
          background: rgba(59, 130, 246, 0.3);
          color: #3b82f6;
        }

        .comment-input-row {
          display: flex;
          gap: 8px;
        }

        .comment-input-row input {
          flex: 1;
          padding: 10px 12px;
          background: rgba(0,0,0,0.3);
          border: 1px solid rgba(255,255,255,0.1);
          border-radius: 6px;
          color: var(--text-primary);
        }

        .comment-input-row button {
          padding: 10px 20px;
          background: #3b82f6;
          color: white;
          border: none;
          border-radius: 6px;
          cursor: pointer;
        }

        .comment-input-row button:disabled {
          opacity: 0.5;
        }

        .comments-list {
          display: flex;
          flex-direction: column;
          gap: 10px;
        }

        .comment-item {
          display: flex;
          gap: 12px;
          padding: 12px;
          background: rgba(255,255,255,0.03);
          border-radius: 8px;
        }

        .comment-icon {
          font-size: 1.2rem;
        }

        .comment-body {
          flex: 1;
        }

        .comment-body p {
          margin: 0 0 4px;
        }

        .comment-time {
          font-size: 0.75rem;
          color: var(--text-secondary);
        }

        .empty {
          text-align: center;
          color: var(--text-secondary);
          padding: 20px;
        }
      `}</style>
    </div>
  );
}

export default MeetingDetailView;
