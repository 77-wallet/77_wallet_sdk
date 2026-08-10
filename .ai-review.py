# ai review程序，勿动

import os
import re
import subprocess
import requests
import smtplib

from email.mime.text import MIMEText
from email.mime.multipart import MIMEMultipart
from email.utils import formataddr
from openai import OpenAI


def send_email(
    to_email,
    user_name,
    project_name,
    branch_name,
    commit_sha,
    commit_title,
    commit_url
):

    smtp_user = os.environ["SMTP_USER"]
    smtp_pass = os.environ["SMTP_PASS"]

    msg = MIMEMultipart()

    msg["From"] = formataddr(
        ("AI Code Review Robot", smtp_user)
    )

    msg["To"] = to_email

    msg["Subject"] = (
        f"[AI Code Review] "
        f"[{project_name}] "
        f"[{branch_name}] "
        f"{commit_title[:60]}"
    )

    body = f"""
您好 {user_name}，

您的代码提交已完成 AI Code Review。

项目：
{project_name}

分支：
{branch_name}

提交标题：
{commit_title}

Commit：
{commit_sha}

AI 审查意见已经发布到 GitLab 评论区。

请点击以下链接查看完整审查结果：

{commit_url}

此邮件为系统自动发送，请勿直接回复。

--
AI Code Review Robot
"""

    msg.attach(
        MIMEText(
            body,
            "plain",
            "utf-8"
        )
    )

    with smtplib.SMTP_SSL(
        "smtp.gmail.com",
        465,
        timeout=30
    ) as server:

        server.login(
            smtp_user,
            smtp_pass
        )

        server.send_message(msg)


def send_telegram(bot_token, chat_id, text):

    resp = requests.post(
        f"https://api.telegram.org/bot{bot_token}/sendMessage",
        data={
            "chat_id": chat_id,
            "text": text,
            "disable_web_page_preview": True
        },
        timeout=30
    )

    resp.raise_for_status()

    return resp


def extract_section(text, header_zh):
    """从形如 '## 标题\n内容...' 的 Markdown 文本中提取某个二级标题下的内容"""

    pattern = rf"##\s*{re.escape(header_zh)}\s*\n(.*?)(?=\n##\s|\Z)"

    match = re.search(pattern, text, re.S)

    if match:
        return match.group(1).strip()

    return ""


def extract_risk_level(review_text):
    """从 '## 风险等级' 小节中提取 高/中/低，取不到则返回未知"""

    section = extract_section(review_text, "风险等级")

    for level in ("高", "中", "低"):
        if level in section:
            return level

    return "未知"


def extract_reason(review_text, max_len=150):
    """从 '## 问题' 小节中提取第一条非空内容，作为简短原因摘要"""

    section = extract_section(review_text, "问题")

    if not section:
        section = review_text

    reason = ""

    for line in section.splitlines():

        line = line.strip(" -*#\t")

        if line:
            reason = line
            break

    if not reason:
        reason = "AI 未给出明确问题描述"

    if len(reason) > max_len:
        reason = reason[:max_len] + "..."

    return reason


client = OpenAI(
    api_key=os.environ["OPENAI_API_KEY"],
    base_url=os.environ["OPENAI_BASE_URL"]
)

print("CI_COMMIT_AUTHOR =", os.environ.get("CI_COMMIT_AUTHOR"))
print("GITLAB_USER_NAME =", os.environ.get("GITLAB_USER_NAME"))
print("GITLAB_USER_EMAIL =", os.environ.get("GITLAB_USER_EMAIL"))

commit_sha = os.environ["CI_COMMIT_SHA"]

branch_name = os.environ.get(
    "CI_COMMIT_REF_NAME",
    "unknown"
)

# 获取 Commit 标题
try:

    commit_title = subprocess.check_output(
        [
            "git",
            "log",
            "-1",
            "--pretty=%s",
            commit_sha
        ],
        text=True
    ).strip()

except Exception:

    commit_title = "No Commit Message"

# 获取本次提交涉及的文件列表（新增/修改的文件，排除被删除的文件）
try:

    changed_files_raw = subprocess.check_output(
        [
            "git",
            "diff-tree",
            "--no-commit-id",
            "--name-status",
            "-r",
            commit_sha
        ],
        stderr=subprocess.STDOUT,
        text=True
    )

except Exception as e:

    print("获取变更文件列表失败:", e)
    exit(1)

changed_files = []

for line in changed_files_raw.splitlines():

    line = line.strip()

    if not line:
        continue

    parts = line.split("\t")

    status = parts[0]

    # 删除的文件没有内容可审查，跳过
    if status.startswith("D"):
        continue

    # 重命名/拷贝状态形如 R100 / C100，文件路径在最后一列
    file_path = parts[-1]

    changed_files.append(file_path)

if not changed_files:

    print("没有代码变更")
    exit(0)

# 常见二进制/非文本文件后缀，跳过审查
BINARY_EXTENSIONS = {
    ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".ico", ".webp",
    ".pdf", ".zip", ".gz", ".tar", ".rar", ".7z",
    ".woff", ".woff2", ".ttf", ".eot", ".otf",
    ".mp3", ".mp4", ".avi", ".mov", ".wav",
    ".so", ".dll", ".dylib", ".exe", ".bin",
    ".jar", ".class", ".pyc",
    ".xlsx", ".xls", ".doc", ".docx", ".ppt", ".pptx",
    ".db", ".sqlite", ".sqlite3",
}

# GPT-5.6 Sol 上下文窗口约 105 万 tokens（约 400 万字符）
# 这里按输入预留 ~60 万 tokens（约 240 万字符）的保守额度，
# 剩余空间留给 prompt 说明文字、输出 tokens 及安全余量。
# 如果实际使用的是更大窗口的版本，可以调大这两个值。

# 单个文件最多提交给模型的字符数，超出部分截断
MAX_FILE_CHARS = 300000

# 单次提交最多审查的文件数量，避免文件数量过多导致请求本身过大/过慢
MAX_REVIEW_FILES = 50

skipped_binary = []
skipped_too_many = []

def is_binary_path(path):

    _, ext = os.path.splitext(path)

    return ext.lower() in BINARY_EXTENSIONS


# 过滤二进制文件
text_files = []

for file_path in changed_files:

    if is_binary_path(file_path):
        skipped_binary.append(file_path)
        continue

    text_files.append(file_path)

# 超出数量上限时，只取前 N 个，其余跳过并提示
if len(text_files) > MAX_REVIEW_FILES:

    skipped_too_many = text_files[MAX_REVIEW_FILES:]
    text_files = text_files[:MAX_REVIEW_FILES]

if skipped_binary:
    print("跳过二进制/非文本文件:", skipped_binary)

if skipped_too_many:
    print(
        f"变更文件数超过 {MAX_REVIEW_FILES}，"
        f"以下文件本次未审查:",
        skipped_too_many
    )

if not text_files:

    print("没有可审查的文本文件")
    exit(0)

# 读取每个变更文件在本次提交时的完整内容
file_contents = []

for file_path in text_files:

    try:

        content = subprocess.check_output(
            [
                "git",
                "show",
                f"{commit_sha}:{file_path}"
            ],
            stderr=subprocess.STDOUT,
            text=True
        )

    except UnicodeDecodeError:

        print(f"文件无法以文本方式解码，跳过 [{file_path}]")
        continue

    except Exception as e:

        print(f"读取文件失败 [{file_path}]:", e)
        continue

    truncated_note = ""

    if len(content) > MAX_FILE_CHARS:

        content = content[:MAX_FILE_CHARS]
        truncated_note = "\n...(内容过长，已截断)..."

    file_contents.append(
        f"===== 文件: {file_path} ====="
        f"\n{content}{truncated_note}"
    )

if not file_contents:

    print("没有可读取的文件内容")
    exit(0)

full_files_text = "\n\n".join(file_contents)

prompt = f"""
你是资深代码审查专家。

请审查以下提交中涉及的完整文件内容（而非仅 diff 片段），
结合整个文件的上下文进行审查：

重点关注：

1. 安全漏洞
2. Bug 风险
3. 性能问题
4. 并发问题
5. 代码规范

输出格式：

## 问题
每个问题里要写上相关代码所在的大概行号

## 风险等级

## 修改建议

变更文件内容：

{full_files_text}
"""

# GPT Review
try:

    resp = client.responses.create(
        model=os.environ["OPENAI_MODEL"],
        input=prompt
    )

    review = resp.output_text

except Exception as e:

    print("GPT 调用失败:", e)
    exit(1)

print("===== AI REVIEW =====")
print(review)

project_id = os.environ.get("CI_PROJECT_ID")
project_name = os.environ.get(
    "CI_PROJECT_NAME",
    "GitLab Project"
)

gitlab_token = os.environ.get("GITLAB_TOKEN")
api_url = os.environ.get("CI_API_V4_URL")
project_url = os.environ.get("CI_PROJECT_URL")

if not all([project_id, gitlab_token, api_url]):

    print("缺少 GitLab 环境变量")
    exit(1)

# 发布 Commit 讨论（Discussion，可回复）
print("CI_API_V4_URL =", api_url)
print("CI_PROJECT_ID =", project_id)
print("CI_COMMIT_SHA =", commit_sha)

try:

    response = requests.post(
        (
            f"{api_url}/projects/{project_id}"
            f"/repository/commits/{commit_sha}/discussions"
        ),
        headers={
            "PRIVATE-TOKEN": gitlab_token
        },
        data={
            "body": review
        },
        timeout=30
    )

    print("GitLab Discussion Status:", response.status_code)
    print("GitLab Discussion Response:", response.text)

    response.raise_for_status()

    result = response.json()
    print("Discussion ID:", result.get("id"))
    print("AI Review 已发布为可回复的讨论线程")

except requests.exceptions.RequestException as exc:

    print("发布 GitLab Discussion 失败:", exc)

    if getattr(exc, "response", None) is not None:
        print("HTTP 状态码:", exc.response.status_code)
        print("响应内容:", exc.response.text)

    raise

print("Review posted successfully")

# Commit 链接，Telegram 通知和邮件通知都会用到
commit_url = ""

if project_url:

    commit_url = (
        f"{project_url}/-/commit/{commit_sha}"
    )

# 项目路径（形如 namespace/project），取不到则退回项目名
project_path = os.environ.get(
    "CI_PROJECT_PATH",
    project_name
)

# 提交人信息（Telegram 通知和邮件通知都会用到，提前获取）
author_name = os.environ.get(
    "GITLAB_USER_NAME"
)

author_email = os.environ.get(
    "GITLAB_USER_EMAIL"
)

print("提交人:", author_name)
print("邮箱:", author_email)

# Telegram 通知（在邮件之前发送）
telegram_bot_token = os.environ.get("TELEGRAM_BOT_TOKEN")
telegram_chat_id = os.environ.get("TELEGRAM_CHAT_ID")

if telegram_bot_token and telegram_chat_id:

    risk_level = extract_risk_level(review)
    reason = extract_reason(review)
    short_sha = commit_sha[:8]

    committer_display = author_name or author_email or "未知"

    telegram_text = (
        f"1. [{risk_level}] {project_path} · commit #{short_sha}\n"
        f"   标题: {commit_title}\n"
        f"   项目: {project_path}\n"
        f"   提交人: {committer_display}\n"
        f"   原因: {reason}\n"
        f"   链接: {commit_url}"
    )

    try:

        print("准备发送 Telegram 通知...")

        send_telegram(
            telegram_bot_token,
            telegram_chat_id,
            telegram_text
        )

        print("Telegram 通知发送成功")

    except Exception as e:

        print("Telegram 通知发送失败:", e)

else:

    print(
        "未配置 TELEGRAM_BOT_TOKEN / TELEGRAM_CHAT_ID，"
        "跳过 Telegram 通知"
    )

# 邮件通知

if not author_email:

    print(
        "未获取到提交人邮箱，跳过邮件发送"
    )
    exit(0)

try:

    print("准备发送邮件...")

    send_email(
        to_email=author_email,
        user_name=author_name or "Developer",
        project_name=project_name,
        branch_name=branch_name,
        commit_sha=commit_sha,
        commit_title=commit_title,
        commit_url=commit_url
    )

    print("邮件发送成功")

except Exception as e:

    print("邮件发送失败:", e)

print("Pipeline Finished")
