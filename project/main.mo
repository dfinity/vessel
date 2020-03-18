import List "mo:list";
import Prim "mo:prim";

func main() {
    let list : List.List<Nat> = #cons{ head = 42; tail = #nil };
    Prim.debugPrintNat(List.headOrElse(list, 0));
};

main()
