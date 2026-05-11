import { AlertTriangle } from "lucide-react";
import { Component, type ErrorInfo, type ReactNode } from "react";

import { Button } from "@/components/ui/button";

interface Props {
  children: ReactNode;
  fallback?: (error: Error, reset: () => void) => ReactNode;
}

interface State {
  error?: Error;
}

export class ErrorBoundary extends Component<Props, State> {
  override state: State = {};

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error("[mxr/error-boundary]", error, info);
  }

  reset = () => this.setState({ error: undefined });

  override render(): ReactNode {
    if (this.state.error) {
      if (this.props.fallback) return this.props.fallback(this.state.error, this.reset);
      return (
        <div className="flex h-full w-full flex-col items-center justify-center gap-3 px-6 text-center">
          <AlertTriangle className="size-6 text-destructive" />
          <div className="text-md font-medium">Something went wrong</div>
          <div className="max-w-sm font-mono text-xs text-muted-foreground">
            {this.state.error.message}
          </div>
          <Button onClick={this.reset} variant="outline" size="sm">
            Try again
          </Button>
        </div>
      );
    }
    return this.props.children;
  }
}
