const items: number[] = [3, 1, 2];
const sorted = items.toSorted((a, b) => a - b);
console.log(JSON.stringify(sorted));
