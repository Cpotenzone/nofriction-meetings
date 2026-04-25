// noFriction Meetings — Reusable EmptyState Component
// Provides a consistent, styled empty state for all views.

import React from 'react';

interface EmptyStateProps {
  icon: string;
  title: string;
  message?: string;
  action?: {
    label: string;
    onClick: () => void;
  };
}

const EmptyState: React.FC<EmptyStateProps> = ({ icon, title, message, action }) => {
  return (
    <div className="nf-empty-state" role="status" aria-label={title}>
      <div className="nf-empty-state__icon">{icon}</div>
      <h3 className="nf-empty-state__title">{title}</h3>
      {message && <p className="nf-empty-state__message">{message}</p>}
      {action && (
        <button
          className="nf-empty-state__action"
          onClick={action.onClick}
          type="button"
        >
          {action.label}
        </button>
      )}
    </div>
  );
};

export default EmptyState;
