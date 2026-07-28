package com.superidm.ui.screens

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowBack
import androidx.compose.material.icons.filled.Assessment
import androidx.compose.material.icons.filled.MilitaryTech
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import com.superidm.data.db.CategoryCount
import com.superidm.data.db.DailyStats
import com.superidm.data.db.DownloadEntity
import com.superidm.data.db.HourlyStats
import com.superidm.util.FormatUtils
import com.superidm.ui.components.formatSpeed
import com.superidm.viewmodel.StatisticsViewModel
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlin.math.cos
import kotlin.math.sin

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun StatisticsScreen(
    viewModel: StatisticsViewModel = hiltViewModel(),
    onBack: () -> Unit
) {
    val totalDownloadedBytes by viewModel.totalDownloadedBytes.collectAsState()
    val averageSpeed by viewModel.averageSpeed.collectAsState()
    val totalCount by viewModel.totalCount.collectAsState()
    val successRate by viewModel.successRate.collectAsState()
    val dailyStats by viewModel.dailyStats.collectAsState()
    val categoryBreakdown by viewModel.categoryBreakdown.collectAsState()
    val hourlyActivity by viewModel.hourlyActivity.collectAsState()
    val topDownloads by viewModel.topDownloads.collectAsState()
    val todayBytes by viewModel.todayBytes.collectAsState()

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Statistics", fontWeight = FontWeight.Bold) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.Default.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    Icon(
                        imageVector = Icons.Default.Assessment,
                        contentDescription = "Chart",
                        modifier = Modifier.padding(end = 16.dp)
                    )
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.primaryContainer,
                    titleContentColor = MaterialTheme.colorScheme.onPrimaryContainer
                )
            )
        }
    ) { paddingValues ->
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(paddingValues)
                .background(MaterialTheme.colorScheme.background),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            // Summary Cards Row
            item {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    SummaryCard(
                        title = "Total Downloaded",
                        value = FormatUtils.formatSize(totalDownloadedBytes),
                        modifier = Modifier.weight(1f)
                    )
                    SummaryCard(
                        title = "Today's Download",
                        value = FormatUtils.formatSize(todayBytes),
                        modifier = Modifier.weight(1f)
                    )
                }
                Spacer(modifier = Modifier.height(8.dp))
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    SummaryCard(
                        title = "Total Downloads",
                        value = "$totalCount total · ${"%.0f".format(successRate)}% success",
                        modifier = Modifier.weight(1f)
                    )
                    SummaryCard(
                        title = "Avg Speed",
                        value = formatSpeed(averageSpeed),
                        modifier = Modifier.weight(1f)
                    )
                }
            }

            // Daily Download Chart
            item {
                ChartCard(title = "Last 30 Days") {
                    DailyDownloadChart(dailyStats = dailyStats)
                }
            }

            // Category Breakdown Donut Chart
            item {
                ChartCard(title = "Category Breakdown") {
                    CategoryDonutChart(categories = categoryBreakdown)
                }
            }

            // Hourly Activity Area Chart
            item {
                ChartCard(title = "Busiest Hours") {
                    HourlyActivityChart(hourlyActivity = hourlyActivity)
                }
            }

            // Top Downloads List
            if (topDownloads.isNotEmpty()) {
                item {
                    Text(
                        text = "Top Downloads by Speed",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.onBackground,
                        modifier = Modifier.padding(top = 8.dp, bottom = 8.dp)
                    )
                }
                itemsIndexed(topDownloads) { index, download ->
                    TopDownloadItem(rank = index + 1, download = download)
                }
            }
        }
    }
}

@Composable
fun SummaryCard(title: String, value: String, modifier: Modifier = Modifier) {
    Card(
        modifier = modifier,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
        shape = RoundedCornerShape(12.dp)
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.Center
        ) {
            Text(
                text = title,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.8f)
            )
            Spacer(modifier = Modifier.height(4.dp))
            Text(
                text = value,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
    }
}

@Composable
fun ChartCard(title: String, content: @Composable () -> Unit) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        elevation = CardDefaults.cardElevation(defaultElevation = 2.dp),
        shape = RoundedCornerShape(16.dp)
    ) {
        Column(
            modifier = Modifier.padding(16.dp)
        ) {
            Text(
                text = title,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.onSurface
            )
            Spacer(modifier = Modifier.height(16.dp))
            content()
        }
    }
}

@Composable
fun DailyDownloadChart(dailyStats: List<DailyStats>) {
    val maxBytes = dailyStats.maxOfOrNull { it.totalBytes }?.toFloat()?.coerceAtLeast(1f) ?: 1f
    
    val animatedProgress by animateFloatAsState(
        targetValue = 1f,
        animationSpec = tween(durationMillis = 1000),
        label = "barChartAnimation"
    )

    Canvas(
        modifier = Modifier
            .fillMaxWidth()
            .height(200.dp)
            .padding(top = 16.dp, bottom = 24.dp)
    ) {
        val width = size.width
        val height = size.height
        
        if (dailyStats.isEmpty()) return@Canvas

        val barWidth = width / (dailyStats.size * 2)
        val space = barWidth

        dailyStats.forEachIndexed { index, stat ->
            val barHeight = (stat.totalBytes.toFloat() / maxBytes) * height * animatedProgress
            val x = index * (barWidth + space) + space / 2

            drawRoundRect(
                brush = Brush.verticalGradient(
                    colors = listOf(Color(0xFF9C27B0), Color(0xFF673AB7))
                ),
                topLeft = Offset(x, height - barHeight),
                size = Size(barWidth, barHeight),
                cornerRadius = androidx.compose.ui.geometry.CornerRadius(4.dp.toPx())
            )
        }
    }
}

@Composable
fun CategoryDonutChart(categories: List<CategoryCount>) {
    val totalCount = categories.sumOf { it.count }.coerceAtLeast(1)
    val colors = listOf(
        Color(0xFFE57373), Color(0xFF81C784), Color(0xFF64B5F6),
        Color(0xFFFFB74D), Color(0xFFBA68C8), Color(0xFF4DB6AC), Color(0xFFA1887F)
    )

    val animatedProgress by animateFloatAsState(
        targetValue = 1f,
        animationSpec = tween(durationMillis = 1000),
        label = "donutAnimation"
    )

    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Canvas(
            modifier = Modifier
                .size(160.dp)
                .padding(8.dp)
        ) {
            var startAngle = -90f
            categories.forEachIndexed { index, category ->
                val sweepAngle = (category.count.toFloat() / totalCount) * 360f * animatedProgress
                val color = colors[index % colors.size]

                drawArc(
                    color = color,
                    startAngle = startAngle,
                    sweepAngle = sweepAngle,
                    useCenter = false,
                    style = Stroke(width = 32.dp.toPx(), cap = StrokeCap.Butt)
                )
                startAngle += sweepAngle
            }
        }

        Spacer(modifier = Modifier.height(16.dp))

        // Legend
        categories.forEachIndexed { index, category ->
            val percentage = (category.count.toFloat() / totalCount) * 100
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(vertical = 4.dp, horizontal = 16.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Box(
                    modifier = Modifier
                        .size(12.dp)
                        .background(colors[index % colors.size], RoundedCornerShape(50))
                )
                Spacer(modifier = Modifier.width(8.dp))
                Text(
                    text = category.categoryFolder.ifEmpty { "Other" },
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.weight(1f)
                )
                Text(
                    text = "${"%.1f".format(percentage)}%",
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.Bold
                )
            }
        }
    }
}

@Composable
fun HourlyActivityChart(hourlyActivity: List<HourlyStats>) {
    val maxCount = hourlyActivity.maxOfOrNull { it.count }?.toFloat()?.coerceAtLeast(1f) ?: 1f
    
    val fullHours = remember(hourlyActivity) {
        List(24) { hour ->
            hourlyActivity.find { it.hour == hour } ?: HourlyStats(hour, 0)
        }
    }

    val animatedProgress by animateFloatAsState(
        targetValue = 1f,
        animationSpec = tween(durationMillis = 1000),
        label = "areaChartAnimation"
    )

    Canvas(
        modifier = Modifier
            .fillMaxWidth()
            .height(150.dp)
            .padding(top = 16.dp, bottom = 8.dp)
    ) {
        val width = size.width
        val height = size.height
        val pointWidth = width / 23f

        val path = Path()
        val fillPath = Path()

        var prevX = 0f
        var prevY = height - (fullHours[0].count.toFloat() / maxCount) * height * animatedProgress

        path.moveTo(prevX, prevY)
        fillPath.moveTo(prevX, height)
        fillPath.lineTo(prevX, prevY)

        for (i in 1 until fullHours.size) {
            val currentX = i * pointWidth
            val currentY = height - (fullHours[i].count.toFloat() / maxCount) * height * animatedProgress
            
            path.lineTo(currentX, currentY)
            fillPath.lineTo(currentX, currentY)
        }

        fillPath.lineTo(width, height)
        fillPath.close()

        drawPath(
            path = fillPath,
            brush = Brush.verticalGradient(
                colors = listOf(Color(0x802196F3), Color.Transparent),
                startY = 0f,
                endY = height
            )
        )

        drawPath(
            path = path,
            color = Color(0xFF2196F3),
            style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round)
        )
    }
}

@Composable
fun TopDownloadItem(rank: Int, download: DownloadEntity) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        shape = RoundedCornerShape(12.dp)
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Box(
                modifier = Modifier
                    .size(40.dp)
                    .background(
                        color = when (rank) {
                            1 -> Color(0xFFFFD700)
                            2 -> Color(0xFFC0C0C0)
                            3 -> Color(0xFFCD7F32)
                            else -> MaterialTheme.colorScheme.primary.copy(alpha = 0.1f)
                        },
                        shape = RoundedCornerShape(50)
                    ),
                contentAlignment = Alignment.Center
            ) {
                if (rank <= 3) {
                    Icon(
                        Icons.Default.MilitaryTech,
                        contentDescription = "Medal",
                        tint = Color.White,
                        modifier = Modifier.size(24.dp)
                    )
                } else {
                    Text(
                        text = "#$rank",
                        style = MaterialTheme.typography.labelMedium,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.primary
                    )
                }
            }

            Spacer(modifier = Modifier.width(16.dp))

            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = download.fileName,
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1
                )
                Text(
                    text = FormatUtils.formatSize(download.totalBytes),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }

            Surface(
                color = MaterialTheme.colorScheme.secondaryContainer,
                shape = RoundedCornerShape(8.dp),
                modifier = Modifier.padding(start = 8.dp)
            ) {
                Text(
                    text = formatSpeed(download.averageSpeed),
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSecondaryContainer,
                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp)
                )
            }
        }
    }
}


