import { render, screen } from "@testing-library/react";
import { StatusPanel } from "./StatusPanel";

describe("StatusPanel", () => {
  it("shows disconnected", () => {
    render(<StatusPanel status={{ state: "disconnected" }} />);
    expect(screen.getByText(/no device/i)).toBeInTheDocument();
  });

  it("shows driver-not-bound guidance", () => {
    render(<StatusPanel status={{ state: "detectedNoDriver" }} />);
    expect(screen.getByText(/driver not bound/i)).toBeInTheDocument();
    expect(screen.getByText(/zadig/i)).toBeInTheDocument();
  });

  it("shows connected SoC name", () => {
    render(
      <StatusPanel
        status={{ state: "connected", soc: { socId: 0x1667, name: "Allwinner R16" } }}
      />,
    );
    expect(screen.getByText(/connected/i)).toBeInTheDocument();
    expect(screen.getByText(/Allwinner R16/)).toBeInTheDocument();
  });
});
