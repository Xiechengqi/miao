import yaml from "yaml";
const config = yaml.parse(await Bun.file("./miao.yaml").text());
const sing_box_home = config.sing_box_home as string;
const direct_sites_link = config.rules.direct_sites_link as string;

console.log(direct_sites_link);

await Bun.write("/tmp/direct.txt", await fetch(direct_sites_link));
gen_direct();


type DomainSet = {
  rules: [
    {
      domain: string[];
      domain_suffix: string[];
      domain_regex: string[];
    },
  ];
  version: number;
};
async function gen_direct() {
  const direct_text = await Bun.file("/tmp/direct.txt").text();
  const direct_items = direct_text.split("\n");

  const direct_set: DomainSet = {
    rules: [
      {
        domain: [],
        domain_suffix: [],
        domain_regex: [],
      },
    ],
    version: 3,
  };

  for (const item of direct_items) {
    if (item.startsWith("full:")) {
      direct_set.rules[0].domain.push(item.replace("full:", ""));
    } else if (item.startsWith("regexp:")) {
      direct_set.rules[0].domain_regex.push(item.replace("regexp:", ""));
    } else {
      if (item) direct_set.rules[0].domain_suffix.push(item);
    }
  }

  await Bun.write("/tmp/direct.json", JSON.stringify(direct_set));
  Bun.spawn({
    cwd: sing_box_home,
    cmd: ["sing-box", "rule-set", "compile", "--output", sing_box_home + "/chinasite.srs", "/tmp/direct.json"],
    env: {
      ...Bun.env,
      PATH: `${Bun.env.PATH}:${sing_box_home}`
    },
    stdout: "inherit",
    stderr: "inherit"
  })
}



