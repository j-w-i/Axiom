import { useState } from "react";
import { Badge } from "../lib/Badge.js";

export function App() {
  const [paid, setPaid] = useState(false);
  return (
    <main className="app" style={{ padding: 24, fontFamily: "sans-serif" }}>
      <h1 className="title">Balance</h1>
      <section className="card" style={{ display: "flex", alignItems: "center", padding: 16 }}>
        <span className="amount">€ 42.00</span>
        <button className="pay" style={{ marginLeft: 8 }} onClick={() => setPaid(true)}>
          Pay now
        </button>
      </section>
      <Badge label={paid ? "Paid" : "Due"} />
    </main>
  );
}
