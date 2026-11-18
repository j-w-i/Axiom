import { Component, type ReactNode } from "react";

/** Contains a failing subtree (e.g. the editor) so the rest of the shell keeps working. */
export class ErrorBoundary extends Component<{ children: ReactNode; label: string }, { error: Error | null }> {
  override state = { error: null as Error | null };
  static getDerivedStateFromError(error: Error) {
    return { error };
  }
  override componentDidCatch(error: Error) {
    console.error(`[${this.props.label}]`, error);
  }
  override render() {
    if (this.state.error) {
      return (
        <div className="empty-editor" role="alert">
          <div style={{ textAlign: "center" }}>
            <p>The {this.props.label} failed: {this.state.error.message}</p>
            <button type="button" className="btn" onClick={() => this.setState({ error: null })}>
              Reload {this.props.label}
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
