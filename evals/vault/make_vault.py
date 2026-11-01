"""Builds the sample vault for agent evals: 39 neutral computer-science, maths and
physics concepts in three areas, with prerequisites, links and a view.

Usage: make_vault.py <new vault dir> <loci binary>
"""
import os, subprocess, sys, json
V = sys.argv[1]
LOCI = os.path.abspath(sys.argv[2])
def run(*a): subprocess.run(a, cwd=V, check=True, capture_output=True)
os.makedirs(V); run("git", "init", "-q")
for k, v in [("commit.gpgsign", "false"), ("core.hooksPath", "/dev/null"), ("user.name", "b"), ("user.email", "b@b")]:
    run("git", "config", k, v)
run(LOCI, "init")
AREAS = {
 "cs": ("Computer science", "Algorithms and data structures, with their costs. Proofs and pure mathematics belong in [[concepts/math/AREA]]; physical systems in [[concepts/physics/AREA]]."),
 "math": ("Mathematics", "Definitions, theorems and proof techniques. Algorithms that use them belong in [[concepts/cs/AREA]]."),
 "physics": ("Physics", "Physical laws and models of motion, energy and waves. The mathematics they rely on belongs in [[concepts/math/AREA]]."),
}
# path -> (title, prereqs, body)
C = {
 "cs/data-structures/array": ("Array", [], "A fixed-size block of memory holding elements of one type, indexed in constant time."),
 "cs/data-structures/linked-list": ("Linked list", [], "Nodes that each hold a value and a pointer to the next node; insertion at a known position is constant time, but indexing is linear."),
 "cs/data-structures/stack": ("Stack", ["linked-list"], "A last-in, first-out collection: push adds on top, pop removes the most recent item. Often built on a [[linked-list]] or an [[array]]."),
 "cs/data-structures/queue": ("Queue", ["linked-list"], "A first-in, first-out collection: items leave in the order they arrived. Used by [[breadth-first-search]]."),
 "cs/data-structures/hash-table": ("Hash table", ["array"], "Maps keys to slots of an [[array]] through a hash function; collisions are resolved by chaining or open addressing. Lookups take expected constant time. An ordered map uses a red-black tree instead."),
 "cs/data-structures/binary-heap": ("Binary heap", ["array"], "A complete binary tree stored in an [[array]] where each parent is no larger than its children; it implements a [[priority-queue]]."),
 "cs/data-structures/priority-queue": ("Priority queue", [], "A collection where the item with the smallest key leaves first. Usually implemented with a [[binary-heap]]."),
 "cs/data-structures/trie": ("Trie (prefix tree)", [], "A tree whose edges are labelled with characters, so that every path from the root spells a prefix; used for autocomplete and dictionary lookups."),
 "cs/data-structures/union-find": ("Disjoint-set forest", [], "Keeps a partition of elements into sets, supporting near-constant-time merging of sets and asking which set an element belongs to. Used by [[kruskals-algorithm]]."),
 "cs/data-structures/graph": ("Graph", [], "A set of vertices joined by edges, which may be directed or weighted. Stored as adjacency lists or an adjacency matrix."),
 "cs/algorithms/breadth-first-search": ("Breadth-first search (BFS)", ["graph", "queue"], "Explores a [[graph]] level by level from a source using a [[queue]]; on unweighted graphs it yields shortest paths in number of edges."),
 "cs/algorithms/depth-first-search": ("Depth-first search (DFS)", ["graph", "stack"], "Explores a [[graph]] by going as deep as possible before backtracking, with recursion or an explicit [[stack]]. Basis of [[topological-sort]]."),
 "cs/algorithms/dijkstras-algorithm": ("Dijkstra's algorithm", ["graph", "priority-queue"], "Finds shortest paths from a source when edge weights are non-negative, repeatedly settling the closest unsettled vertex taken from a [[priority-queue]] and relaxing its edges. A* search extends it with an estimate of the remaining distance."),
 "cs/algorithms/bellman-ford": ("Bellman-Ford algorithm", ["graph"], "Finds shortest paths from a source even with negative edge weights, by relaxing every edge V-1 times; a further improving pass reveals a negative cycle."),
 "cs/algorithms/topological-sort": ("Topological sort", ["depth-first-search"], "Orders the vertices of a directed acyclic graph so that every edge points forward; computed from finishing times of [[depth-first-search]]."),
 "cs/algorithms/kruskals-algorithm": ("Kruskal's algorithm", ["union-find", "graph"], "Builds a minimum spanning tree by taking edges in order of weight and skipping any that would close a cycle, which a [[union-find]] detects."),
 "cs/algorithms/binary-search": ("Binary search", ["array"], "Finds a key in a sorted [[array]] by halving the interval at each step, in logarithmic time. A red-black tree keeps a set searchable as it changes."),
 "cs/algorithms/merge-sort": ("Merge sort", ["divide-and-conquer"], "A stable sort that splits the input in half, sorts each half and merges them, in O(n log n) time and linear extra space. An instance of [[divide-and-conquer]]."),
 "cs/algorithms/quicksort": ("Quicksort", ["divide-and-conquer"], "Partitions around a pivot and sorts the parts recursively; O(n log n) on average, quadratic in the worst case, and in place."),
 "cs/algorithms/divide-and-conquer": ("Divide and conquer", ["recurrence-relation"], "Solves a problem by splitting it into smaller instances, solving them, and combining the results; its cost is a [[recurrence-relation]]."),
 "cs/algorithms/dynamic-programming": ("Dynamic programming", ["recurrence-relation"], "Solves overlapping subproblems once each and stores their answers, either top-down with memoisation or bottom-up in a table."),
 "cs/algorithms/big-o-notation": ("Big-O notation", ["limit"], "Describes how a cost grows with input size, ignoring constant factors: f is O(g) when f stays below a constant multiple of g for large inputs."),
 "math/proof-by-induction": ("Proof by induction", [], "Prove a base case, then that each case implies the next; the statement then holds for every natural number."),
 "math/recurrence-relation": ("Recurrence relation", [], "An equation defining each term of a sequence from earlier terms, such as T(n) = 2T(n/2) + n; solved by unrolling, guessing and checking with [[proof-by-induction]], or the master theorem."),
 "math/limit": ("Limit", [], "The value a function approaches as its input approaches a point; the foundation of [[derivative]] and [[integral]]."),
 "math/derivative": ("Derivative", ["limit"], "The instantaneous rate of change of a function, defined as a [[limit]] of difference quotients; the slope of the tangent line."),
 "math/integral": ("Integral", ["limit"], "The accumulated area under a curve, defined as a [[limit]] of sums; inverse to the [[derivative]] by the fundamental theorem of calculus."),
 "math/linear-algebra/vector-space": ("Vector space", [], "A set closed under addition and scalar multiplication, satisfying the usual axioms; its elements are vectors."),
 "math/linear-algebra/matrix": ("Matrix", ["vector-space"], "A rectangular array of numbers representing a linear map between [[vector-space]]s; multiplication composes the maps."),
 "math/linear-algebra/eigenvalue": ("Eigenvalue", ["matrix"], "A scalar λ such that a [[matrix]] A maps some nonzero vector v to λv; v is an eigenvector. Found as roots of the characteristic polynomial, the determinant of A minus λ times the identity."),
 "math/probability/bayes-theorem": ("Bayes' theorem", ["conditional-probability"], "Turns P(B|A) into P(A|B) by weighting with the prior P(A) and dividing by P(B); the rule for updating a belief on evidence."),
 "math/probability/conditional-probability": ("Conditional probability", [], "The probability of an event given that another has occurred: P(A|B) = P(A and B) / P(B)."),
 "math/probability/expected-value": ("Expected value", [], "The probability-weighted average of a random variable's outcomes; linear even for dependent variables."),
 "physics/newtons-second-law": ("Newton's second law", ["derivative"], "Force equals mass times acceleration, F = ma, where acceleration is the [[derivative]] of velocity. Equivalently, force is the rate of change of momentum."),
 "physics/kinetic-energy": ("Kinetic energy", ["newtons-second-law"], "The energy of motion, one half of mass times speed squared; the work needed to bring a body to its speed."),
 "physics/conservation-of-energy": ("Conservation of energy", ["kinetic-energy"], "In an isolated system the total energy stays constant; it only changes form, for example between [[kinetic-energy]] and potential energy."),
 "physics/simple-harmonic-motion": ("Simple harmonic motion", ["newtons-second-law"], "Oscillation under a restoring force proportional to displacement, as in a mass on a spring; the position is a sine of time."),
 "physics/waves/wave-equation": ("Wave equation", ["derivative"], "A partial differential equation relating the second time derivative of a displacement to its second space derivative; its solutions travel at a fixed speed."),
 "physics/waves/doppler-effect": ("Doppler effect", ["wave-equation"], "The change in observed frequency when a source and an observer move relative to each other: higher when approaching, lower when receding."),
}
for a, (t, p) in AREAS.items():
    os.makedirs(f"{V}/concepts/{a}", exist_ok=True)
    open(f"{V}/concepts/{a}/AREA.md", "w").write(f"# {t}\n\n{p}\n")
for path, (title, pre, body) in C.items():
    out = subprocess.run([LOCI, "--json", "new", f"concepts/{path}", "--title", title], cwd=V, capture_output=True, text=True)
    assert out.returncode == 0, out.stdout
    f = f"{V}/concepts/{path}.md"
    s = open(f).read()
    plist = ", ".join(f'"[[{x}]]"' for x in pre)
    s = s.replace("prereqs: []", f"prereqs: [{plist}]") + f"\n{body}\n"
    open(f, "w").write(s)
os.makedirs(f"{V}/views", exist_ok=True)
open(f"{V}/views/shortest-paths.md", "w").write("# Shortest paths\n\nStart with [[breadth-first-search]], then [[dijkstras-algorithm]], then [[bellman-ford]]. A* search comes last.\n")
run("git", "add", "-A"); run("git", "commit", "-qm", "bench vault")
r = subprocess.run([LOCI, "lint"], cwd=V, capture_output=True, text=True)
assert r.returncode == 0, r.stdout + r.stderr
