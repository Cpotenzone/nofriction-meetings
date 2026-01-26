
import { Component, ErrorInfo, ReactNode } from "react";

interface Props {
    children: ReactNode;
}

interface State {
    hasError: boolean;
    error: Error | null;
    errorInfo: ErrorInfo | null;
}

export class GlobalErrorBoundary extends Component<Props, State> {
    public state: State = {
        hasError: false,
        error: null,
        errorInfo: null,
    };

    public static getDerivedStateFromError(error: Error): State {
        return { hasError: true, error, errorInfo: null };
    }

    public componentDidCatch(error: Error, errorInfo: ErrorInfo) {
        console.error("Uncaught error:", error, errorInfo);
        this.setState({ error, errorInfo });
    }

    public render() {
        if (this.state.hasError) {
            return (
                <div style={{
                    padding: "40px",
                    background: "#1a1d29",
                    color: "#e5e7eb",
                    height: "100vh",
                    fontFamily: "system-ui, sans-serif",
                    display: "flex",
                    flexDirection: "column",
                    gap: "20px"
                }}>
                    <div>
                        <span style={{ fontSize: "40px" }}>💥</span>
                        <h1 style={{ fontSize: "24px", fontWeight: "bold", margin: "10px 0" }}>Something went wrong</h1>
                        <p style={{ color: "#9ca3af" }}>The application encountered an error and cannot display.</p>
                    </div>

                    <div style={{
                        background: "rgba(0,0,0,0.3)",
                        padding: "20px",
                        borderRadius: "8px",
                        overflow: "auto",
                        border: "1px solid rgba(255,255,255,0.1)",
                        color: "#ff8888",
                        fontFamily: "monospace",
                        fontSize: "13px"
                    }}>
                        <p style={{ fontWeight: "bold", marginBottom: "10px" }}>{this.state.error?.toString()}</p>
                        <pre style={{ whiteSpace: "pre-wrap" }}>{this.state.errorInfo?.componentStack}</pre>
                    </div>

                    <button
                        onClick={() => window.location.reload()}
                        style={{
                            padding: "10px 20px",
                            background: "#7c3aed",
                            color: "white",
                            border: "none",
                            borderRadius: "8px",
                            fontWeight: 600,
                            cursor: "pointer",
                            alignSelf: "flex-start"
                        }}
                    >
                        Reload Application
                    </button>
                </div>
            );
        }

        return this.props.children;
    }
}
