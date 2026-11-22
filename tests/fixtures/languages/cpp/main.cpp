#include <vector>
#include <algorithm>
#include <iostream>

int main() {
    std::vector<int> v{3, 1, 2};
    std::sort(v.begin(), v.end());
    std::cout << v.front() << "\n";
}
