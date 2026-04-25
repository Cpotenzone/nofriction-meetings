// noFriction Meetings — Reusable ErrorState Component
// Provides a consistent, styled error state with optional retry action.

import React from 'react';

interface ErrorStateProps {
  title?: string;
  message: string;
  onRetry?: () => void;
  retryLabel?: string;
}

const ErrorState: React.FC<ErrorStateProps> = ({
  title = 'Something went wrong',
  message,
  onRetry,
  retryLabel = 'Try Again',
}) => {
  return (
    <div className="nf-error-state" role="alert">
      <div className="nf-error-state__icon">⚠️</div>
      <h3 className="nf-error-state__title">{title}</h3>
      <p className="nf-error-state__message">{message}</p>
      {onRetry && (
        <button
          className="nf-error-state__action"
          onClick={onRetry}
          type="button"
        >
          {retryLabel}
        </button>
      )}
    </div>
  );
};

export default ErrorState;
