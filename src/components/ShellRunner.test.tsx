import { render, screen } from "@testing-library/react";
import { ShellRunner } from "./ShellRunner";

describe("ShellRunner", () => {
  it("disabled when not connected", () => {
    render(<ShellRunner connected={false} />);
    expect(screen.getByRole("button", { name: /run.*uname/i })).toBeDisabled();
  });

  it("shows stdout when done", () => {
    render(
      <ShellRunner
        connected={true}
        initialProgress={{ phase: "done", stdout: "Linux clover 4.4.0", exitCode: 0 }}
      />,
    );
    expect(screen.getByText(/Linux clover 4\.4\.0/)).toBeInTheDocument();
  });
});
