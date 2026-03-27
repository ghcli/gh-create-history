# gh-create-history

> "how many commits do you need?"
> "a hundred thousand."
> "by when?"
> "thursday."

A GitHub CLI extension that generates synthetic git history at mass production speed. Branches, merges, octopus merges, conflict resolutions, file renames, deletes, tags — the full drama of a real repository, manufactured in under a minute.

Written in Rust. Powered by libgit2. No working directory. No index. Just raw object creation directly into the git object store, because shelling out to `git commit` a hundred thousand times is what we call a distributed denial of service on yourself.

---

## Install

```bash
gh extension install ghcli/gh-create-history
```

Or build from source if you trust no one:

```bash
git clone https://github.com/ghcli/gh-create-history.git
cd gh-create-history
cargo install --path .
```

---

## The Menu

```
gh create-history [OPTIONS]

  --commits <N>        Commits per branch                        [default: 1000]
  --branches <N>       Number of branches to create              [default: 100]
  --size <SIZE>        Max file size: 512b, 1kb, 10mb            [default: 1kb]
  --oldest <DURATION>  Spread commits over: 1yr, 6mo, 30d, 2w   [default: 1yr]
  --push               Push everything to origin when done
  --seed <N>           RNG seed for reproducible runs
  --files <N>          Files to touch per commit                 [default: 1-5]
  --repo-path <PATH>   Target repo                               [default: cwd]
  --quiet              Shut up and generate
```

---

## Real Use Cases

### 1. Your GHES instance needs a stress test and your boss needs it by thursday

You just deployed GitHub Enterprise Server. Everything works great with 3 repos and 47 commits. But production will have thousands of repos with millions of commits. Will it hold? Will the search index choke? Will the backup job finish before the heat death of the universe?

```bash
mkdir ghes-stress && cd ghes-stress && git init
git remote add origin https://ghes.yourcompany.com/load-test/big-repo.git

gh create-history \
  --commits 1000 \
  --branches 100 \
  --size 10kb \
  --oldest 2yr \
  --push
```

You now have 100,000+ commits across 100 branches with 2 years of realistic history, pushed to your GHES instance. The backup job will have something to think about tonight.

### 2. Your CI pipeline claims it can handle anything

Your Jenkins/Actions/CircleCI pipeline says "optimized for large repos." Sure. Prove it.

```bash
mkdir ci-torture && cd ci-torture && git init
git remote add origin git@github.com:yourorg/ci-stress-test.git

gh create-history \
  --commits 500 \
  --branches 50 \
  --size 1kb \
  --oldest 1yr \
  --seed 42 \
  --push
```

Now trigger a build. Watch your CI queue. Watch the shallow clone time. Watch the checkout step go from 2 seconds to "is this thing frozen?" That is the information you needed.

The `--seed 42` means you can reproduce this exact repo tomorrow when your CI team says "we fixed it" and you want to verify.

### 3. Testing git mirrors, replicas, and backup tools

You are evaluating a git mirroring tool. The vendor says it handles "repos of any size." You have heard that before.

```bash
# generate the source repo
mkdir mirror-source && cd mirror-source
gh create-history \
  --commits 200 \
  --branches 30 \
  --size 5kb \
  --oldest 6mo \
  --quiet

# set up the mirror and push
git remote add origin git@github.com:yourorg/mirror-test-source.git
git push --all && git push --tags

# now point your mirror tool at it and see what happens
```

30 branches. Merge commits. Octopus merges. Tags. File renames across branches. If the mirror tool survives this, it will survive your monorepo. Probably.

### 4. Benchmarking git operations before a migration

You are migrating from BitBucket/GitLab/SVN to GitHub. The migration tool estimates "2 hours." You have also heard that before.

```bash
# generate repos of increasing size to find the breaking point
for size in 100 500 1000 5000; do
  dir="bench-${size}"
  mkdir "$dir"
  gh create-history \
    --commits "$size" \
    --branches 20 \
    --size 1kb \
    --oldest 1yr \
    --seed 42 \
    --repo-path "$dir" \
    --quiet
  echo "$dir: $(cd $dir && git rev-list --all --count) commits"
done
```

```
bench-100: 2,100 commits
bench-500: 10,500 commits
bench-1000: 21,000 commits
bench-5000: 105,000 commits
```

Now run your migration tool against each one. Plot the time. Find the cliff. Schedule the maintenance window accordingly. Not based on the vendor estimate. Based on data.

### 5. Load testing GitHub Apps, webhooks, and integrations

Your GitHub App processes push events. It works fine when someone pushes 3 commits. What happens when someone force-pushes a branch with 500 commits? What happens when 10 branches get pushed simultaneously?

```bash
mkdir webhook-stress && cd webhook-stress && git init
git remote add origin git@github.com:yourorg/webhook-stress.git

# generate a repo with many branches
gh create-history \
  --commits 100 \
  --branches 20 \
  --size 512b \
  --oldest 30d \
  --seed 99

# push all branches at once — your webhook endpoint will feel this
git push --all
git push --tags
```

20 push events hitting your webhook endpoint in rapid succession. Each with 100+ commits. If your App queues properly, great. If it drops events, you just found out in staging instead of production. You are welcome.

### 6. Reproducing "works on small repos, breaks on large repos" bugs

A customer reports that code search is slow on their repo. Their repo has 50,000 commits across 200 branches. You cannot clone their repo. You can build one that looks like it.

```bash
mkdir repro-customer-issue && cd repro-customer-issue

gh create-history \
  --commits 250 \
  --branches 200 \
  --size 1kb \
  --oldest 3yr \
  --seed 7

# now test code search, blame, git log, whatever was slow
time git log --all --oneline | wc -l
time git log --all --follow -- "src/auth/handler.rs"
time git shortlog -sn --all
```

Same shape. Same scale. Reproducible with `--seed 7`. Ship the seed to your colleague instead of a 2GB tarball.

### 7. Testing your git GUI before it meets a real repo

Your team built a git visualization tool. It renders beautiful branch graphs. With 5 branches. What happens with 50?

```bash
mkdir gui-test && cd gui-test

gh create-history \
  --commits 30 \
  --branches 50 \
  --size 512b \
  --oldest 90d \
  --seed 42
```

50 branches. Octopus merges. Branches that fork from other branches. If your rendering engine survives this without a stack overflow or a UI that looks like a plate of spaghetti, congratulations. You built something real.

### 8. A/B testing git server configurations

You are tuning git server settings — pack window size, delta cache, gc thresholds. You need identical repos to compare against.

```bash
# same seed = same repo, byte for byte
gh create-history --commits 500 --branches 50 --seed 42 --repo-path config-a --quiet
gh create-history --commits 500 --branches 50 --seed 42 --repo-path config-b --quiet

# verify they are identical
diff <(cd config-a && git rev-parse HEAD) <(cd config-b && git rev-parse HEAD)
# no output = identical
```

Now apply different server configs to each. Benchmark. Compare. The repos are identical so any performance difference is your config, not the data.

---

## What Gets Generated

This is not random garbage. It is structured random garbage.

| Feature | What It Looks Like |
|---------|-------------------|
| **Branches** | `feature/branch-1`, `feature/branch-42` — realistic branch topology |
| **Merges** | Feature branches merge back to main with 2-parent merge commits |
| **Octopus merges** | ~5% of merges combine 3+ branches. Yes, people do this |
| **Merge conflicts** | ~10% of merges have simulated conflict resolution markers |
| **File renames** | Files get renamed across commits, visible in `git log --follow` |
| **File deletes** | Some files get removed as branches evolve |
| **Tags** | Milestone tags at regular intervals |
| **Timestamps** | Weekday-heavy, business-hours bias, realistic jitter |
| **File content** | Source code, markdown, JSON, YAML — looks real to the eye |
| **Directories** | `src/`, `tests/`, `docs/`, `config/` — nested 1-3 levels |

---

## How Fast Is It

It uses libgit2 to write objects directly to the object store. No working directory. No index. No shell-outs.

| Scenario | Commits | Branches | Total Objects | Time |
|----------|---------|----------|---------------|------|
| Smoke test | 10 | 3 | ~50 | < 1s |
| Medium | 100 | 10 | ~1,100 | ~5s |
| Large | 1,000 | 50 | ~51,000 | ~30s |
| Stress | 1,000 | 100 | ~101,000 | ~60s |

Your mileage may vary. But it will vary fast.

---

## Verifying the Output

Every generated repo is a real git repo. Verify it with the commands you already know.

```bash
# branches
git branch --list | wc -l

# total commits across all branches
git rev-list --all --count

# merge commits
git log --all --merges --oneline | wc -l

# octopus merges (3+ parents)
git log --all --min-parents=3 --oneline | wc -l

# file renames
git log --all --diff-filter=R --name-status | grep ^R | wc -l

# conflict resolutions
git log --all --grep="Resolve merge conflict" --oneline | wc -l

# is the repo healthy
git fsck --full --no-dangling
```

Or run the included eval script that checks all of the above in one shot:

```bash
tests/eval/verify.sh ./my-repo --commits 1000 --branches 100 --size 1024 --oldest 365
```

It outputs TAP format. 14 checks. Pass or fail. No opinions.

---

## Why Rust

Because generating 100,000 git objects in Python would take long enough to reconsider your career choices. Rust plus libgit2 equals memory-safe speed without the existential dread.

Also the error messages are better than "Segmentation fault (core dumped)" which is what you get when you try to do this in C.

---

## Contributing

PRs welcome. If you find a way to make it faster, I owe you a mass produced beer.

## License

MIT. Do whatever you want with it. Generate a million commits. I do not judge.
