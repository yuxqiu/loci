"""Lengths of Rust functions in crates/*/src (outside test modules): prints the longest and summary stats."""
import re, subprocess, sys
files = [f for f in subprocess.check_output(['git', 'ls-files', '-co', '--exclude-standard', 'crates']).decode().split() if '/src/' in f and f.endswith('.rs')]
lens = []
for f in files:
    lines = open(f).read().split('\n')
    stop = next((i for i, l in enumerate(lines) if l.strip() == '#[cfg(test)]'), len(lines))
    i = 0
    while i < stop:
        m = re.match(r'^(\s*)(pub(\([a-z]+\))? )?fn (\w+)', lines[i])
        if m and not lines[i].rstrip().endswith(';'):
            indent = m.group(1)
            j = i
            while j < stop and not (lines[j].startswith(indent + '}') and len(lines[j].rstrip()) == len(indent) + 1):
                j += 1
            lens.append((j - i + 1, f'{f}:{i+1} {m.group(4)}'))
            i = j
        i += 1
lens.sort(reverse=True)
n = len(lens)
print(f'functions {n}, over 40 lines {sum(1 for l,_ in lens if l > 40)}, over 60 {sum(1 for l,_ in lens if l > 60)}, longest {lens[0][0]}, mean {sum(l for l,_ in lens)/n:.1f}')
for l, name in lens[:int(sys.argv[1]) if len(sys.argv) > 1 else 12]:
    print(l, name)
