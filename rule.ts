try {
  await Bun.write(
    "./files/direct.txt",
    await fetch(
      "https://raw.githubusercontent.com/Loyalsoldier/v2ray-rules-dat/release/direct-list.txt",
    ),
  );
  await Bun.write(
    "./files/cnip.txt",
    await fetch(
      "https://raw.githubusercontent.com/Loyalsoldier/clash-rules/release/cncidr.txt",
    ),
  );
} catch (e) {
  console.error("ERROR on fetching rules.");
}

Promise.all([gen_direct(), gen_cnip()]);

async function gen_cnip() {
  const cnip_text = await Bun.file("./files/cnip.txt").text();
  let arr = cnip_text.split("\n");
  console.log(arr[0]);
  arr.pop();
  arr.shift();
  arr = arr.map((x) => x.substring(x.indexOf("'") + 1, x.lastIndexOf("'")));
  const rule_set = {
    version: 3,
    rules: [{ ip_cidr: [] }],
  } as any;
  for (const x of arr) {
    rule_set.rules[0].ip_cidr.push(x);
  }
  await Bun.write("./files/cnip.json", JSON.stringify(rule_set));
  await Bun.$`./sing-box/sing-box rule-set compile --output ./sing-box/chinaip.srs ./files/cnip.json`;
}

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
  const direct_text = await Bun.file("./files/direct.txt").text();
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

  await Bun.write("./files/direct.json", JSON.stringify(direct_set));
  await Bun.$`./sing-box/sing-box rule-set compile --output ./sing-box/chinasite.srs ./files/direct.json`;
}
