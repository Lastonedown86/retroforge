import { render, screen } from "@testing-library/react";
import { MembootButton } from "./MembootButton";

describe("MembootButton", () => {
  it("is disabled when device not connected", () => {
    render(<MembootButton connected={false} />);
    expect(screen.getByRole("button", { name: /memboot/i })).toBeDisabled();
  });

  it("is enabled when connected", () => {
    render(<MembootButton connected={true} />);
    expect(screen.getByRole("button", { name: /memboot/i })).toBeEnabled();
  });

  it("shows a progress label when a phase is set", () => {
    render(<MembootButton connected={true} initialPhase={{ phase: "initDram" }} />);
    expect(screen.getByText(/init.*dram/i)).toBeInTheDocument();
  });
});
