import BalanceCard from "./components/BalanceCard.jsx";

const accounts = [
  { id: "chq", name: "Cheque", balance: 1204.5 },
  { id: "sav", name: "Savings", balance: 98311.07 },
];

export default function App() {
  return (
    <main style={{ padding: 20, width: 380 }}>
      <h1 style={{ fontSize: 20 }}>Accounts</h1>
      {accounts.map((a) => (
        <BalanceCard key={a.id} name={a.name} balance={a.balance} />
      ))}
    </main>
  );
}
