package fixture;

import java.util.List;

public class Main {
    public static void main(String[] args) {
        List<String> names = List.of("a", "b");
        System.out.println(String.join(",", names));
    }
}
