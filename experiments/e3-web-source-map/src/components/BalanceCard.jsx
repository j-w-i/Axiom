export default function BalanceCard({ name, balance }) {
  return (
    <section style={{ background: "#fff", borderRadius: 8, padding: 12, marginBottom: 12 }}>
      <h2 style={{ fontSize: 14, margin: 0 }}>{name}</h2>
      <p style={{ fontSize: 24, margin: "4px 0" }}>${balance.toFixed(2)}</p>
      <button style={{ marginLeft: 0, padding: "6px 12px" }}>Transfer</button>
    </section>
  );
}
