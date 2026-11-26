import { Component, type ErrorInfo, type ReactNode } from 'react';
import { ArrowClockwise, WarningCircle } from '@phosphor-icons/react';

interface ErrorBoundaryProps {
  paneName?: string;
  children: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = {
    hasError: false,
    error: null,
  };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(`[ErrorBoundary:${this.props.paneName ?? 'pane'}]`, error, info);
  }

  private handleReload = () => {
    this.setState({ hasError: false, error: null });
  };

  render() {
    if (!this.state.hasError) {
      return this.props.children;
    }

    const label = this.props.paneName || 'this pane';
    const message = this.state.error?.message || 'An unexpected rendering error occurred.';

    return (
      <div
        className="flex-1 h-full w-full flex items-center justify-center p-6"
        style={{ backgroundColor: 'var(--bg-app)', color: 'var(--text-main)' }}
      >
        <div
          className="sakana-card max-w-md w-full rounded-[12px] border p-5 space-y-3"
          style={{ borderColor: 'var(--border-line)', backgroundColor: 'var(--bg-card)' }}
        >
          <div className="flex items-center gap-2">
            <WarningCircle size={18} style={{ color: 'var(--accent-red)' }} />
            <h2 className="text-[14px] font-semibold m-0" style={{ color: 'var(--text-main)' }}>
              Failed to render {label}
            </h2>
          </div>
          <p
            className="font-mono-tech text-[11.5px] m-0 break-words rounded-[8px] border p-2.5"
            style={{
              color: 'var(--text-body)',
              borderColor: 'var(--border-line)',
              backgroundColor: 'var(--bg-elevated)',
            }}
          >
            {message}
          </p>
          <div className="flex justify-end">
            <button
              type="button"
              onClick={this.handleReload}
              className="btn-solid px-3 py-1.5 text-[12px] font-medium inline-flex items-center gap-1.5 cursor-pointer"
            >
              <ArrowClockwise size={13} />
              <span>Reload pane</span>
            </button>
          </div>
        </div>
      </div>
    );
  }
}
