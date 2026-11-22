using System.Linq;

var numbers = new[] { 3, 1, 2 };
System.Console.WriteLine(numbers.OrderBy(n => n).First());
